use appstruct_generated_backend::{
    Job, JobHandler, JobHandlerError, JobWorker, MailJobPayload, MailState, RequestContext,
};
use async_trait::async_trait;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use std::{collections::BTreeMap, env, sync::Arc};

struct TestHandler;

#[async_trait]
impl JobHandler for TestHandler {
    async fn handle(&self, job: &Job) -> Result<(), JobHandlerError> {
        match job.kind.as_str() {
            "succeed" => Ok(()),
            "fail" => Err(JobHandlerError("x".repeat(2_500))),
            kind => Err(JobHandlerError(format!("unsupported test job `{kind}`"))),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = Database::connect(env::var("DATABASE_URL")?).await?;
    let tenant_id = env::var("TENANT_ID")?.parse::<uuid::Uuid>()?;
    let mail = MailState::from_env(database.clone())?;
    clear(&database).await?;

    assert_transactional_enqueue(&database, &mail, tenant_id).await?;
    assert_success_and_deduplication(&database, &mail, tenant_id).await?;
    assert_retry_and_dead_state(&database, &mail, tenant_id).await?;
    assert_expired_lease_recovery(&database, &mail, tenant_id).await?;
    assert_mail_job(&database, &mail, tenant_id).await?;

    let handle = JobWorker::new(database.clone(), Arc::new(TestHandler)).spawn();
    handle.shutdown().await;
    seed_admin_jobs(&database, &mail, tenant_id).await?;
    Ok(())
}

async fn seed_admin_jobs(
    database: &DatabaseConnection,
    mail: &MailState,
    tenant_id: uuid::Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    clear(database).await?;
    let context = RequestContext::connection(database, mail, None, Some(tenant_id));
    let succeeded = context
        .enqueue_job("default", "succeed", &serde_json::json!({"admin": true}), None, None)
        .await?;
    let dead = context
        .enqueue_job("default", "fail", &serde_json::json!({"admin": true}), None, None)
        .await?;
    let success_worker = JobWorker::for_kind(database.clone(), Arc::new(TestHandler), "succeed");
    assert!(success_worker.run_once().await?);
    let failure_worker = JobWorker::for_kind(database.clone(), Arc::new(TestHandler), "fail");
    assert!(failure_worker.run_once().await?);
    database
        .execute_unprepared(&format!(
            "UPDATE \"_appstruct_jobs\" SET run_at = CURRENT_TIMESTAMP WHERE id = '{}'::uuid",
            dead.id
        ))
        .await?;
    assert!(failure_worker.run_once().await?);
    assert_eq!(job_state(database, succeeded.id).await?, "succeeded|1");
    assert_eq!(job_state(database, dead.id).await?, "dead|2");
    Ok(())
}

async fn assert_transactional_enqueue(
    database: &DatabaseConnection,
    mail: &MailState,
    tenant_id: uuid::Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    let transaction = database.begin().await?;
    let context = RequestContext::transaction(&transaction, mail, None, Some(tenant_id));
    context
        .enqueue_job(
            "default",
            "succeed",
            &serde_json::json!({"source": "rollback"}),
            Some("rollback"),
            None,
        )
        .await?;
    drop(context);
    transaction.rollback().await?;
    assert_eq!(count_jobs(database).await?, 0);
    Ok(())
}

async fn assert_success_and_deduplication(
    database: &DatabaseConnection,
    mail: &MailState,
    tenant_id: uuid::Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    let transaction = database.begin().await?;
    let context = RequestContext::transaction(&transaction, mail, None, Some(tenant_id));
    let first = context
        .enqueue_job(
            "default",
            "succeed",
            &serde_json::json!({"source": "commit"}),
            Some("committed"),
            None,
        )
        .await?;
    let duplicate = context
        .enqueue_job(
            "default",
            "succeed",
            &serde_json::json!({"source": "duplicate"}),
            Some("committed"),
            None,
        )
        .await?;
    assert_eq!(first.id, duplicate.id);
    assert!(!first.deduplicated);
    assert!(duplicate.deduplicated);
    drop(context);
    transaction.commit().await?;

    assert_eq!(count_jobs(database).await?, 1);
    assert_eq!(job_tenant(database, first.id).await?, tenant_id);
    let worker = JobWorker::new(database.clone(), Arc::new(TestHandler));
    assert!(worker.run_once().await?);
    assert_eq!(job_state(database, first.id).await?, "succeeded|1");
    assert!(!worker.run_once().await?);
    Ok(())
}

async fn assert_retry_and_dead_state(
    database: &DatabaseConnection,
    mail: &MailState,
    tenant_id: uuid::Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    clear(database).await?;
    let context = RequestContext::connection(database, mail, None, Some(tenant_id));
    let receipt = context
        .enqueue_job("default", "fail", &(), None, None)
        .await?;
    let worker = JobWorker::new(database.clone(), Arc::new(TestHandler));
    assert!(worker.run_once().await?);
    assert_eq!(job_state(database, receipt.id).await?, "queued|1");
    assert!(retry_is_delayed(database, receipt.id).await?);
    assert_eq!(last_error_length(database, receipt.id).await?, 2_000);

    database
        .execute_unprepared(&format!(
            "UPDATE \"_appstruct_jobs\" SET run_at = CURRENT_TIMESTAMP WHERE id = '{}'::uuid",
            receipt.id
        ))
        .await?;
    assert!(worker.run_once().await?);
    assert_eq!(job_state(database, receipt.id).await?, "dead|2");
    assert!(completed(database, receipt.id).await?);
    Ok(())
}

async fn assert_expired_lease_recovery(
    database: &DatabaseConnection,
    mail: &MailState,
    tenant_id: uuid::Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    clear(database).await?;
    let context = RequestContext::connection(database, mail, None, Some(tenant_id));
    let receipt = context
        .enqueue_job("default", "succeed", &(), None, None)
        .await?;
    database
        .execute_unprepared(&format!(
            "UPDATE \"_appstruct_jobs\" SET status = 'running', locked_by = 'lost-worker', locked_until = CURRENT_TIMESTAMP - INTERVAL '1 second' WHERE id = '{}'::uuid",
            receipt.id
        ))
        .await?;
    let worker = JobWorker::new(database.clone(), Arc::new(TestHandler));
    assert!(worker.run_once().await?);
    assert_eq!(job_state(database, receipt.id).await?, "succeeded|1");
    Ok(())
}

async fn assert_mail_job(
    database: &DatabaseConnection,
    mail: &MailState,
    tenant_id: uuid::Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    clear(database).await?;
    database
        .execute_unprepared("DELETE FROM \"_appstruct_mail_deliveries\"")
        .await?;
    let context = RequestContext::connection(database, mail, None, Some(tenant_id));
    let payload = MailJobPayload {
        template: "job-complete".to_owned(),
        recipient: "jobs@example.com".to_owned(),
        variables: BTreeMap::from([("job_name".to_owned(), "Import".to_owned())]),
    };
    let receipt = context
        .enqueue_job("mail", "mail.send", &payload, Some("mail-job"), None)
        .await?;
    for _ in 0..40 {
        if job_state(database, receipt.id).await? == "succeeded|1" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(job_state(database, receipt.id).await?, "succeeded|1");
    assert_eq!(capture_count(database).await?, 1);
    Ok(())
}

async fn clear(database: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    database.execute_unprepared("DELETE FROM \"_appstruct_jobs\"").await?;
    Ok(())
}

async fn count_jobs(database: &DatabaseConnection) -> Result<i64, sea_orm::DbErr> {
    scalar_i64(database, "SELECT COUNT(*)::bigint AS value FROM \"_appstruct_jobs\"").await
}

async fn capture_count(database: &DatabaseConnection) -> Result<i64, sea_orm::DbErr> {
    scalar_i64(
        database,
        "SELECT COUNT(*)::bigint AS value FROM \"_appstruct_mail_deliveries\"",
    )
    .await
}

async fn last_error_length(
    database: &DatabaseConnection,
    id: uuid::Uuid,
) -> Result<i64, sea_orm::DbErr> {
    scalar_i64(database, &format!(
        "SELECT char_length(last_error)::bigint AS value FROM \"_appstruct_jobs\" WHERE id = '{id}'::uuid"
    )).await
}

async fn job_state(
    database: &DatabaseConnection,
    id: uuid::Uuid,
) -> Result<String, sea_orm::DbErr> {
    scalar_string(database, &format!(
        "SELECT status || '|' || attempts::text AS value FROM \"_appstruct_jobs\" WHERE id = '{id}'::uuid"
    )).await
}

async fn job_tenant(
    database: &DatabaseConnection,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, sea_orm::DbErr> {
    let row = query(database, &format!(
        "SELECT tenant_id AS value FROM \"_appstruct_jobs\" WHERE id = '{id}'::uuid"
    )).await?;
    row.try_get("", "value")
}

async fn retry_is_delayed(
    database: &DatabaseConnection,
    id: uuid::Uuid,
) -> Result<bool, sea_orm::DbErr> {
    scalar_bool(database, &format!(
        "SELECT run_at > CURRENT_TIMESTAMP AS value FROM \"_appstruct_jobs\" WHERE id = '{id}'::uuid"
    )).await
}

async fn completed(
    database: &DatabaseConnection,
    id: uuid::Uuid,
) -> Result<bool, sea_orm::DbErr> {
    scalar_bool(database, &format!(
        "SELECT completed_at IS NOT NULL AS value FROM \"_appstruct_jobs\" WHERE id = '{id}'::uuid"
    )).await
}

async fn scalar_i64(database: &DatabaseConnection, sql: &str) -> Result<i64, sea_orm::DbErr> {
    query(database, sql).await?.try_get("", "value")
}

async fn scalar_string(
    database: &DatabaseConnection,
    sql: &str,
) -> Result<String, sea_orm::DbErr> {
    query(database, sql).await?.try_get("", "value")
}

async fn scalar_bool(database: &DatabaseConnection, sql: &str) -> Result<bool, sea_orm::DbErr> {
    query(database, sql).await?.try_get("", "value")
}

async fn query(
    database: &DatabaseConnection,
    sql: &str,
) -> Result<sea_orm::QueryResult, sea_orm::DbErr> {
    database
        .query_one_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await?
        .ok_or_else(|| sea_orm::DbErr::Custom("query returned no row".to_owned()))
}
