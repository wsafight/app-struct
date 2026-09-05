use appstruct_generated_backend::{Actor, ApiError, MailState, RequestContext, entities::project};
use sea_orm::{ColumnTrait, Condition, ConnectionTrait, Database, DbBackend, EntityTrait, QueryFilter, QuerySelect, Statement, TransactionTrait};
use serde_json::Value;
use uuid::Uuid;

fn access_denied(context: &RequestContext<'_>) -> ApiError {
    if context.actor().is_some() { ApiError::Forbidden } else { ApiError::Unauthorized }
}

fn allows(rule: &Value, actor: Option<&Actor>, owner: Uuid) -> bool {
    match rule["mode"].as_str().unwrap() {
        "public" => true,
        "authenticated" => actor.is_some(),
        "role" => actor.is_some_and(|actor| actor.roles.iter().any(|role| role == rule["role"].as_str().unwrap())),
        "owner" => actor.is_some_and(|actor| actor.id == owner),
        "any" => rule["rules"].as_array().unwrap().iter().any(|rule| allows(rule, actor, owner)),
        "all" => rule["rules"].as_array().unwrap().iter().all(|rule| allows(rule, actor, owner)),
        other => panic!("unknown rule {other}"),
    }
}

#[tokio::test]
async fn postgres_policy_matrix() {
    let Ok(url) = std::env::var("APPSTRUCT_ACCESS_TEST_DATABASE_URL") else {
        eprintln!("PostgreSQL execution requires APPSTRUCT_ACCESS_TEST_DATABASE_URL; generated contract still compiles");
        return;
    };
    let database = Database::connect(url).await.unwrap();
    let mail = MailState::from_env(database.clone()).unwrap();
    let transaction = database.begin().await.unwrap();
    transaction.execute_unprepared("CREATE TEMP TABLE projects (id uuid, owner_id uuid, tenant_id uuid, deleted_at timestamptz) ON COMMIT DROP").await.unwrap();
    let tenants = [Uuid::from_u128(1), Uuid::from_u128(2)];
    let owners = [Uuid::from_u128(3), Uuid::from_u128(4)];
    let mut rows = Vec::new();
    for tenant in tenants {
        for owner in owners {
            for deleted in [false, true] {
                let id = Uuid::from_u128(100 + rows.len() as u128);
                transaction.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                    "INSERT INTO projects VALUES ($1, $2, $3, CASE WHEN $4 THEN CURRENT_TIMESTAMP ELSE NULL END)",
                    [id.into(), owner.into(), tenant.into(), deleted.into()])).await.unwrap();
                rows.push((id, tenant, owner, deleted));
            }
        }
    }
    let actors = [None, Some(Actor { id: owners[0], email: "member@example.test".to_owned(), roles: vec!["member".to_owned()] }),
        Some(Actor { id: owners[1], email: "other@example.test".to_owned(), roles: vec!["member".to_owned()] }),
        Some(Actor { id: owners[0], email: "admin@example.test".to_owned(), roles: vec!["admin".to_owned()] })];
    let rules = rules();
    let mut comparisons = 0;
    for actor in actors {
        for tenant in [None, Some(tenants[0]), Some(tenants[1])] {
            let context = RequestContext::transaction(&transaction, &mail, actor.clone(), tenant);
            for (index, rule) in rules.as_array().unwrap().iter().enumerate() {
                for mode in 0..4 {
                    let mut actual = match select_case(index, mode, &context) {
                        Ok(select) => select.select_only().column(project::Column::Id).into_tuple::<Uuid>().all(&transaction).await.unwrap(),
                        Err(ApiError::Unauthorized | ApiError::Forbidden | ApiError::InvalidTenant) => Vec::new(),
                        Err(error) => panic!("unexpected error {error:?}"),
                    };
                    let mut expected = rows.iter().filter(|(_, row_tenant, owner, deleted)|
                        tenant == Some(*row_tenant) && *deleted == (mode == 2) && allows(rule, actor.as_ref(), *owner)
                    ).map(|(id, _, _, _)| *id).collect::<Vec<_>>();
                    actual.sort(); expected.sort();
                    assert_eq!(actual, expected, "rule={rule}, mode={mode}, tenant={tenant:?}, actor={actor:?}");
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 2112);
    transaction.rollback().await.unwrap();
}
