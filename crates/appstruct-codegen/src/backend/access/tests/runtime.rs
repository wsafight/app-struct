use appstruct_generated_backend::{Actor, ApiError, MailState, RequestContext, entities::project};
use appstruct_generated_backend::database::{Database, DatabaseExecutor, Sql};
use sea_query::Condition;
use sqlx::Row as _;
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
                transaction.execute(Sql::bind(
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
                        Ok(select) => transaction.fetch_all(select.select_only().column(project::Column::Id).statement()).await.unwrap().iter().map(|row| row.try_get::<Uuid, _>("id").unwrap()).collect::<Vec<_>>(),
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

#[tokio::test]
async fn postgres_sqlx_binds_and_transaction_ownership() {
    use appstruct_generated_backend::database::Sql;
    let Ok(url) = std::env::var("APPSTRUCT_ACCESS_TEST_DATABASE_URL") else { return; };
    let database = Database::connect(url).await.unwrap();
    let mail = MailState::from_env(database.clone()).unwrap();
    let table = format!("appstruct_sqlx_probe_{}", Uuid::now_v7().simple());
    database.execute_unprepared(&format!(
        "CREATE TABLE {table} (id integer PRIMARY KEY, value text, optional bigint, amount numeric, payload jsonb, day date, owner uuid)"
    )).await.unwrap();
    let insert = format!("INSERT INTO {table} VALUES ($1, $2, $3, $4, $5, $6, $7)");
    let owner = Uuid::now_v7();
    let amount = rust_decimal::Decimal::from_str_exact("9007199254740993.123456789").unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    let statement = |id: i32| Sql::bind(&insert, [
        id.into(), "'; DROP TABLE projects; --".into(), Option::<i64>::None.into(),
        amount.into(), serde_json::json!({"nested": [true, 42]}).into(), day.into(), owner.into(),
    ]);
    let count_sql = format!("SELECT count(*) AS count FROM {table}");
    let transaction = database.begin().await.unwrap();
    {
        let context = RequestContext::transaction(&transaction, &mail, None, None);
        context.database().execute(statement(1)).await.unwrap();
        let row = context.database().fetch_optional(Sql::new(format!("SELECT * FROM {table}"))).await.unwrap().unwrap();
        assert_eq!(row.try_get::<String, _>("value").unwrap(), "'; DROP TABLE projects; --");
        assert_eq!(row.try_get::<Option<i64>, _>("optional").unwrap(), None);
        assert_eq!(row.try_get::<rust_decimal::Decimal, _>("amount").unwrap(), amount);
        assert_eq!(row.try_get::<serde_json::Value, _>("payload").unwrap(), serde_json::json!({"nested": [true, 42]}));
        assert_eq!(row.try_get::<chrono::NaiveDate, _>("day").unwrap(), day);
        assert_eq!(row.try_get::<Uuid, _>("owner").unwrap(), owner);
    }
    // A database error is isolated to its savepoint; the outer transaction stays usable.
    let failed = transaction.begin().await.unwrap();
    let error = failed.execute(statement(1)).await.unwrap_err();
    assert!(error.as_database_error().unwrap().is_unique_violation());
    failed.rollback().await.unwrap();
    let succeeded = transaction.begin().await.unwrap();
    succeeded.execute(statement(2)).await.unwrap();
    succeeded.commit().await.unwrap();
    // Cancellation/drop queues rollback before the next operation, including commit.
    let abandoned = transaction.begin().await.unwrap();
    abandoned.execute(statement(3)).await.unwrap();
    drop(abandoned);
    transaction.commit().await.unwrap();
    let count: i64 = database.fetch_optional(Sql::new(&count_sql)).await.unwrap().unwrap().try_get("count").unwrap();
    assert_eq!(count, 2);
    let abandoned = database.begin().await.unwrap();
    abandoned.execute(statement(4)).await.unwrap();
    drop(abandoned);
    let count: i64 = database.fetch_optional(Sql::new(count_sql)).await.unwrap().unwrap().try_get("count").unwrap();
    assert_eq!(count, 2);
    database.execute_unprepared(&format!("DROP TABLE {table}")).await.unwrap();
}

#[derive(Clone, Debug, sqlx::FromRow, sift_query::SearchEntity)]
#[search_entity(from = "appstruct_sift_probe", default_order = "id:asc", stable_order = "id:asc")]
struct SiftProbe {
    #[search_field(sortable, cursor_non_null, operators = [Eq])]
    id: i64,
    #[sqlx(rename = "label")]
    #[search_field(column = "label", operators = [Eq])]
    name: String,
    #[search_field(operators = [Eq, Ge, Le])]
    score: Option<i32>,
    #[search_field(filterable = false)]
    payload: serde_json::Value,
}

#[tokio::test]
async fn postgres_sift_filters_preserve_bound_queries_and_paging() {
    use appstruct_generated_backend::database::{Select, sift_condition};
    use sea_query::Expr;
    use sift_query::Operator;
    let Ok(url) = std::env::var("APPSTRUCT_ACCESS_TEST_DATABASE_URL") else { return; };
    let database = Database::connect(url).await.unwrap();
    let transaction = database.begin().await.unwrap();
    transaction.execute_unprepared("CREATE TEMP TABLE appstruct_sift_probe (id bigint, label text, score integer, payload jsonb) ON COMMIT DROP").await.unwrap();
    transaction.execute(Sql::bind("INSERT INTO appstruct_sift_probe VALUES (1, $1, 10, '{}'), (2, 'second', 20, '{}'), (3, 'null', NULL, '{}')", ["'; DROP TABLE appstruct_sift_probe; --".into()])).await.unwrap();
    let query = Select::<SiftProbe>::new("appstruct_sift_probe")
        .filter(sift_condition(SiftProbeFields::SCORE, 10, Operator::Ge).unwrap())
        .filter(sift_condition(SiftProbeFields::SCORE, 20, Operator::Le).unwrap());
    assert_eq!(query.count(&transaction).await.unwrap(), 2);
    let page = query.order_by_asc(Expr::col(sea_query::Alias::new("id"))).limit(1).offset(1).all(&transaction).await.unwrap();
    assert_eq!(page[0].id, 2);
    assert_eq!(page[0].score, Some(20));
    assert_eq!(page[0].payload, serde_json::json!({}));
    let rows = Select::<SiftProbe>::new("appstruct_sift_probe")
        .filter(sift_condition(SiftProbeFields::NAME, "'; DROP TABLE appstruct_sift_probe; --".to_owned(), Operator::Eq).unwrap())
        .all(&transaction).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, 1);
    assert_eq!(rows[0].name, "'; DROP TABLE appstruct_sift_probe; --");
    assert!(matches!(sift_condition(SiftProbeFields::ID, 2, Operator::Ge), Err(ApiError::InvalidQuery(_))));
    transaction.rollback().await.unwrap();
}
