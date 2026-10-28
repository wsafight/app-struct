mod lifecycle {
    use super::*;
    use std::{collections::BTreeMap, sync::OnceLock};

    #[derive(Default)]
    struct AuditPolicy {
        redactions: BTreeMap<String, Vec<String>>,
        retention_days: Option<u32>,
    }

    static POLICY: OnceLock<Result<AuditPolicy, String>> = OnceLock::new();

    fn read_policy() -> &'static Result<AuditPolicy, String> {
        POLICY.get_or_init(|| {
            let redactions = optional_env("APPSTRUCT_AUDIT_REDACT_FIELDS")?;
            let retention = optional_env("APPSTRUCT_AUDIT_RETENTION_DAYS")?;
            parse_policy(redactions.as_deref(), retention.as_deref())
        })
    }

    fn optional_env(name: &str) -> Result<Option<String>, String> {
        match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(format!("{name} must contain valid Unicode")),
        }
    }

    fn parse_policy(redactions: Option<&str>, retention: Option<&str>) -> Result<AuditPolicy, String> {
        let redactions: BTreeMap<String, Vec<String>> = redactions
            .map(serde_json::from_str).transpose()
            .map_err(|_| "APPSTRUCT_AUDIT_REDACT_FIELDS must be an object of entity names and JSON pointer arrays".to_owned())?
            .unwrap_or_default();
        for (entity, paths) in &redactions {
            if entity.is_empty() || paths.len() > 256 {
                return Err("audit redaction entities must be named and have at most 256 paths".to_owned());
            }
            for path in paths {
                if !path.starts_with('/') || path.len() > 1024 || path.split('/').skip(1).any(|part| {
                    let mut chars = part.chars();
                    while let Some(ch) = chars.next() {
                        if ch == '~' && !matches!(chars.next(), Some('0' | '1')) { return true; }
                    }
                    false
                }) {
                    return Err("audit redaction paths must be valid, bounded JSON pointers".to_owned());
                }
            }
        }
        let retention_days = retention.map(|days| days.parse::<u32>())
            .transpose().map_err(|_| "APPSTRUCT_AUDIT_RETENTION_DAYS must be an integer".to_owned())?;
        if retention_days.is_some_and(|days| !(1..=36500).contains(&days)) {
            return Err("APPSTRUCT_AUDIT_RETENTION_DAYS must be between 1 and 36500".to_owned());
        }
        Ok(AuditPolicy { redactions, retention_days })
    }

    pub fn validate_env() -> Result<(), String> {
        read_policy().as_ref().map(|_| ()).map_err(Clone::clone)
    }

    fn redact_value(policy: &AuditPolicy, entity: &str, value: &mut serde_json::Value) {
        for paths in [policy.redactions.get("*"), policy.redactions.get(entity)].into_iter().flatten() {
            for path in paths {
                if let Some(value) = value.pointer_mut(path) {
                    *value = serde_json::Value::String("[REDACTED]".to_owned());
                }
            }
        }
    }

    pub(super) fn redact(entity: &str, mut value: Option<serde_json::Value>) -> Result<Option<serde_json::Value>, ApiError> {
        let policy = read_policy().as_ref().map_err(|_| ApiError::Internal)?;
        if let Some(value) = value.as_mut() { redact_value(policy, entity, value); }
        Ok(value)
    }

    /// Explicit maintenance operation. No audit events are removed unless retention is configured.
    /// Run only from a trusted maintenance job, after any required archive has been verified.
    pub async fn prune_expired<C: DatabaseExecutor>(database: &C, batch_size: u32) -> Result<u64, ApiError> {
        if !(1..=10000).contains(&batch_size) {
            return Err(ApiError::InvalidQuery("audit cleanup batch_size must be between 1 and 10000".to_owned()));
        }
        let Some(days) = read_policy().as_ref().map_err(|_| ApiError::Internal)?.retention_days else { return Ok(0); };
        let result = database.execute(Sql::bind(
            r#"WITH expired AS (
                SELECT id FROM "_appstruct_audit_events"
                WHERE occurred_at < CURRENT_TIMESTAMP - $1 * INTERVAL '1 day'
                ORDER BY occurred_at, id LIMIT $2 FOR UPDATE SKIP LOCKED
            ) DELETE FROM "_appstruct_audit_events" events USING expired WHERE events.id = expired.id"#,
            [i32::try_from(days).unwrap_or(36500).into(), i64::from(batch_size).into()],
        )).await?;
        Ok(result.rows_affected())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn redaction_handles_nested_fields_arrays_and_pointer_escaping() {
            let policy = parse_policy(Some(r#"{"*":["/email"],"app::Customer":["/profile/phone","/items/0/token","/a~1b"]}"#), Some("30")).unwrap();
            let mut value = serde_json::json!({"email":"secret", "profile":{"phone":"123","name":"keep"}, "items":[{"token":"hidden"}],"a/b":"private"});
            redact_value(&policy, "app::Customer", &mut value);
            assert_eq!(value["email"], "[REDACTED]");
            assert_eq!(value["profile"]["phone"], "[REDACTED]");
            assert_eq!(value["items"][0]["token"], "[REDACTED]");
            assert_eq!(value["a/b"], "[REDACTED]");
            assert_eq!(value["profile"]["name"], "keep");
            assert!(parse_policy(Some(r#"{"*":["/bad~path"]}"#), None).is_err());
            assert!(parse_policy(Some("[]"), None).is_err());
            assert!(parse_policy(None, Some("0")).is_err());
            assert!(parse_policy(None, Some("36501")).is_err());
        }
    }
}

pub use lifecycle::{prune_expired, validate_env};
