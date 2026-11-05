/// One object reported by a storage provider listing.
#[derive(Clone, Debug)]
pub struct StoredObject {
    pub key: String,
    pub size: u64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileMode {
    /// Count and list orphans only. Nothing is deleted.
    Report,
    /// Delete orphans, at most `max_deletes` attempts per pass.
    Delete,
}

#[derive(Clone, Debug)]
pub struct ReconcileOptions {
    pub mode: ReconcileMode,
    /// Objects modified more recently than this are never touched (1 hour minimum).
    pub min_age: std::time::Duration,
    /// Keys looked up per database query, 1..=1000.
    pub batch_size: usize,
    /// Required in `Delete` mode, 1..=10000 attempts. Ignored in `Report` mode.
    pub max_deletes: usize,
    /// Orphan keys kept in the report, at most 1000. Counts are always exact.
    pub max_listed: usize,
}

impl Default for ReconcileOptions {
    fn default() -> Self {
        Self {
            mode: ReconcileMode::Report,
            min_age: std::time::Duration::from_secs(24 * 60 * 60),
            batch_size: 500,
            max_deletes: 0,
            max_listed: 100,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReconcileReport {
    /// Objects seen in storage.
    pub scanned: u64,
    /// Skipped because they were modified within `min_age`.
    pub too_young: u64,
    /// Skipped because the key is not a safe AppStruct key (not ours to judge).
    pub foreign_keys: u64,
    /// Objects with no metadata row and older than `min_age`.
    pub orphans: u64,
    pub deleted: u64,
    pub delete_failures: u64,
    pub orphan_keys: Vec<String>,
}
