impl FileState {
    /// Register a business owner in the caller's transaction after authorizing that record.
    pub async fn retain(&self, key: &str, entity: &str, record_id: &str, tenant: Option<uuid::Uuid>) -> Result<(), FileError> {
        self.retain_with_connection(&self.database, key, entity, record_id, tenant).await
    }

    pub(crate) async fn retain_with_connection<C: DatabaseExecutor>(&self, database: &C, key: &str, entity: &str, record_id: &str, tenant: Option<uuid::Uuid>) -> Result<(), FileError> {
        validate_key(key)?;
        let owner = reference_owner(entity, record_id)?;
        let result = database.execute(Sql::bind(
            r#"UPDATE "_appstruct_files" SET
                reference_owners = CASE WHEN reference_owners @> $3::jsonb THEN reference_owners ELSE reference_owners || $3::jsonb END,
                unreferenced_at = NULL
            WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2 AND NOT deleting
                AND (reference_owners @> $3::jsonb OR jsonb_array_length(reference_owners) < 1000)"#,
            [key.to_owned().into(), tenant.into(), owner.into()],
        )).await?;
        if result.rows_affected() == 0 { return Err(FileError::ReferenceConflict); }
        Ok(())
    }

    pub async fn release(&self, key: &str, entity: &str, record_id: &str, tenant: Option<uuid::Uuid>) -> Result<(), FileError> {
        self.release_with_connection(&self.database, key, entity, record_id, tenant).await
    }

    pub(crate) async fn release_with_connection<C: DatabaseExecutor>(&self, database: &C, key: &str, entity: &str, record_id: &str, tenant: Option<uuid::Uuid>) -> Result<(), FileError> {
        validate_key(key)?;
        let owner = reference_owner(entity, record_id)?;
        let result = database.execute(Sql::bind(
            r#"UPDATE "_appstruct_files" SET
                reference_owners = (SELECT COALESCE(jsonb_agg(item), '[]'::jsonb) FROM jsonb_array_elements(reference_owners) item WHERE NOT $3::jsonb @> jsonb_build_array(item)),
                unreferenced_at = CASE WHEN reference_owners = '[]'::jsonb THEN COALESCE(unreferenced_at, CURRENT_TIMESTAMP) WHEN reference_owners <@ $3::jsonb THEN CURRENT_TIMESTAMP ELSE NULL END
            WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2 AND NOT deleting"#,
            [key.to_owned().into(), tenant.into(), owner.into()],
        )).await?;
        if result.rows_affected() == 0 { return Err(FileError::ReferenceConflict); }
        Ok(())
    }

    /// Trusted maintenance only. Never scheduled automatically. Storage deletes must be idempotent.
    /// Failed objects stay marked for retry; they cannot acquire new references.
    pub async fn collect_unreferenced(&self, retention_days: u32, batch_size: u32) -> Result<u64, FileError> {
        if !(1..=36500).contains(&retention_days) || !(1..=100).contains(&batch_size) {
            return Err(FileError::Configuration("file retention must be 1..=36500 days and batch_size 1..=100".to_owned()));
        }
        let transaction = self.database.begin().await?;
        let rows = transaction.fetch_all(Sql::bind(
            __APPSTRUCT_FILE_COLLECTION_SQL__,
            [i32::try_from(retention_days).unwrap_or(36500).into(), i64::from(batch_size).into()],
        )).await?;
        let mut deleted = 0;
        let mut failure = None;
        for row in rows {
            let key: String = row.try_get("object_key")?;
            match self.remove_claimed(&transaction, &key).await {
                Ok(()) => deleted += 1,
                Err(error) => { if failure.is_none() { failure = Some(error); } }
            }
        }
        transaction.commit().await?;
        if let Some(error) = failure { return Err(error); }
        Ok(deleted)
    }

    async fn remove_claimed<C: DatabaseExecutor>(&self, database: &C, key: &str) -> Result<(), FileError> {
        self.provider.delete(key).await?;
        database.execute(Sql::bind(
            "DELETE FROM \"_appstruct_files\" WHERE object_key = $1 AND deleting AND reference_owners = '[]'::jsonb",
            [key.to_owned().into()],
        )).await?;
        Ok(())
    }
}

impl FileState {
    /// Trusted maintenance only. Never scheduled automatically. Finds storage objects that have no
    /// `_appstruct_files` row. Report mode is the default and changes nothing. Listing order is not
    /// guaranteed and a pass cannot be resumed, so an interrupted pass starts over.
    pub async fn reconcile_storage(&self, options: ReconcileOptions) -> Result<ReconcileReport, FileError> {
        let minimum = std::time::Duration::from_secs(60 * 60);
        if options.min_age < minimum
            || !(1..=1000).contains(&options.batch_size)
            || options.max_listed > 1000
            || (options.mode == ReconcileMode::Delete && !(1..=10000).contains(&options.max_deletes))
        {
            return Err(FileError::Configuration(
                "reconcile needs min_age >= 1h, batch_size 1..=1000, max_listed <= 1000 and, in delete mode, max_deletes 1..=10000".to_owned(),
            ));
        }
        let age = chrono::Duration::from_std(options.min_age)
            .map_err(|error| FileError::Configuration(error.to_string()))?;
        let cutoff = chrono::Utc::now().checked_sub_signed(age)
            .ok_or_else(|| FileError::Configuration("reconcile min_age exceeds the supported timestamp range".to_owned()))?;
        let mut report = ReconcileReport::default();
        let mut batch: Vec<String> = Vec::with_capacity(options.batch_size);
        let mut stream = self.provider.list();
        while let Some(item) = stream.next().await {
            let object = item?;
            report.scanned += 1;
            if object.last_modified > cutoff { report.too_young += 1; continue; }
            if validate_key(&object.key).is_err() { report.foreign_keys += 1; continue; }
            batch.push(object.key);
            if batch.len() >= options.batch_size {
                self.reconcile_batch(&options, &mut report, std::mem::take(&mut batch)).await?;
            }
        }
        if !batch.is_empty() {
            self.reconcile_batch(&options, &mut report, batch).await?;
        }
        Ok(report)
    }

    async fn reconcile_batch(&self, options: &ReconcileOptions, report: &mut ReconcileReport, keys: Vec<String>) -> Result<(), FileError> {
        let known = self.known_keys(&keys).await?;
        for key in keys.into_iter().filter(|key| !known.contains(key)) {
            report.orphans += 1;
            if report.orphan_keys.len() < options.max_listed { report.orphan_keys.push(key.clone()); }
            if options.mode != ReconcileMode::Delete || report.deleted + report.delete_failures >= options.max_deletes as u64 {
                continue;
            }
            // A late commit may have created the row since the batch lookup. Check again, then
            // delete. This narrows the race but cannot close it; `min_age` is the real guard.
            if !self.known_keys(std::slice::from_ref(&key)).await?.is_empty() { continue; }
            match self.provider.delete(&key).await {
                Ok(()) => report.deleted += 1,
                Err(_) => report.delete_failures += 1,
            }
        }
        Ok(())
    }

    async fn known_keys(&self, keys: &[String]) -> Result<HashSet<String>, FileError> {
        let rows = self.database.fetch_all(Sql::bind(
            r#"SELECT object_key FROM "_appstruct_files" WHERE object_key IN (SELECT jsonb_array_elements_text($1::jsonb))"#,
            [serde_json::json!(keys).into()],
        )).await?;
        rows.into_iter().map(|row| row.try_get("object_key").map_err(FileError::from)).collect()
    }
}

fn reference_owner(entity: &str, record_id: &str) -> Result<serde_json::Value, FileError> {
    if [entity, record_id].iter().any(|value| value.is_empty() || value.len() > 255 || value.chars().any(char::is_control)) {
        return Err(FileError::Configuration("file owner entity and record_id must contain 1..=255 bytes without control characters".to_owned()));
    }
    Ok(serde_json::json!([{ "entity": entity, "record_id": record_id }]))
}
