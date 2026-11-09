use appstruct_generated_backend::{FileError, FileState, ReconcileMode, ReconcileOptions};

#[tokio::test]
async fn reconciliation_returns_disabled_with_the_same_options() {
    assert!(matches!(FileState::default().stream("missing", None).await, Err(FileError::Disabled)));
    let options = ReconcileOptions {
        mode: ReconcileMode::Delete,
        max_deletes: 1,
        ..ReconcileOptions::default()
    };
    assert!(matches!(
        FileState::default().reconcile_storage(options).await,
        Err(FileError::Disabled)
    ));
}
