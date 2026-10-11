// PostgreSQL execution shared by generated services and transactional hooks.
use async_trait::async_trait;
use sea_query::Value;
pub use sqlx::{
    Error as DbErr,
    postgres::{PgQueryResult, PgRow},
};
use sqlx::{PgPool, Postgres, Row};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

pub struct Sql {
    pub sql: String,
    pub values: Vec<Value>,
}
impl Sql {
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            values: Vec::new(),
        }
    }
    pub fn bind(sql: impl Into<String>, values: impl IntoIterator<Item = Value>) -> Self {
        Self {
            sql: sql.into(),
            values: values.into_iter().collect(),
        }
    }
    fn query(
        &self,
    ) -> Result<sqlx::query::Query<'_, Postgres, sqlx::postgres::PgArguments>, DbErr> {
        let mut query = sqlx::query(sqlx::AssertSqlSafe(self.sql.as_str()));
        for value in self.values.clone() {
            query = match value {
                Value::Bool(v) => query.bind(v),
                Value::TinyInt(v) => query.bind(v.map(i16::from)),
                Value::SmallInt(v) => query.bind(v),
                Value::Int(v) => query.bind(v),
                Value::BigInt(v) => query.bind(v),
                Value::TinyUnsigned(v) => query.bind(v.map(i16::from)),
                Value::SmallUnsigned(v) => query.bind(v.map(i32::from)),
                Value::Unsigned(v) => query.bind(v.map(i64::from)),
                Value::BigUnsigned(v) => query.bind(
                    v.map(i64::try_from)
                        .transpose()
                        .map_err(|e| DbErr::Protocol(e.to_string()))?,
                ),
                Value::Float(v) => query.bind(v),
                Value::Double(v) => query.bind(v),
                Value::String(v) => query.bind(v),
                Value::Char(v) => query.bind(v.map(|c| c.to_string())),
                Value::Bytes(v) => query.bind(v),
                Value::Json(v) => query.bind(v.map(|v| *v)),
                Value::ChronoDate(v) => query.bind(v),
                Value::ChronoTime(v) => query.bind(v),
                Value::ChronoDateTime(v) => query.bind(v),
                Value::ChronoDateTimeUtc(v) => query.bind(v),
                Value::ChronoDateTimeLocal(v) => query.bind(v),
                Value::ChronoDateTimeWithTimeZone(v) => query.bind(v),
                Value::Uuid(v) => query.bind(v),
                Value::Decimal(v) => query.bind(v),
                _ => return Err(DbErr::Protocol("unsupported SQL bind type".to_owned())),
            };
        }
        Ok(query)
    }
}

#[async_trait]
pub trait DatabaseExecutor: Send + Sync {
    async fn execute(&self, sql: Sql) -> Result<PgQueryResult, DbErr>;
    async fn fetch_optional(&self, sql: Sql) -> Result<Option<PgRow>, DbErr>;
    async fn fetch_all(&self, sql: Sql) -> Result<Vec<PgRow>, DbErr>;
    async fn execute_unprepared(&self, sql: &str) -> Result<PgQueryResult, DbErr>;
}

#[derive(Clone, Debug)]
pub struct Database {
    pool: PgPool,
}
impl Database {
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn ping(&self) -> Result<(), DbErr> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    pub async fn connect(url: impl AsRef<str>) -> Result<Self, DbErr> {
        Ok(Self::from_pool(PgPool::connect(url.as_ref()).await?))
    }
    pub async fn begin(&self) -> Result<Transaction, DbErr> {
        Ok(Transaction {
            shared: Arc::new(TransactionConnection {
                connection: tokio::sync::Mutex::new(Some(self.pool.begin().await?)),
                pending: Mutex::new(Vec::new()),
                sequence: AtomicU64::new(0),
            }),
            savepoint: None,
        })
    }
}
#[async_trait]
impl DatabaseExecutor for Database {
    async fn execute(&self, sql: Sql) -> Result<PgQueryResult, DbErr> {
        sql.query()?.execute(&self.pool).await
    }
    async fn fetch_optional(&self, sql: Sql) -> Result<Option<PgRow>, DbErr> {
        sql.query()?.fetch_optional(&self.pool).await
    }
    async fn fetch_all(&self, sql: Sql) -> Result<Vec<PgRow>, DbErr> {
        sql.query()?.fetch_all(&self.pool).await
    }
    async fn execute_unprepared(&self, sql: &str) -> Result<PgQueryResult, DbErr> {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(&self.pool).await
    }
}
struct TransactionConnection {
    connection: tokio::sync::Mutex<Option<sqlx::Transaction<'static, Postgres>>>,
    pending: Mutex<Vec<String>>,
    sequence: AtomicU64,
}
pub struct Transaction {
    shared: Arc<TransactionConnection>,
    savepoint: Option<String>,
}
impl Transaction {
    async fn connection(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<sqlx::Transaction<'static, Postgres>>>, DbErr>
    {
        let mut guard = self.shared.connection.lock().await;
        let connection = guard
            .as_mut()
            .ok_or_else(|| DbErr::Protocol("transaction is closed".to_owned()))?;
        let pending = std::mem::take(
            &mut *self
                .shared
                .pending
                .lock()
                .expect("savepoint queue poisoned"),
        );
        for sql in pending {
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str())).execute(&mut **connection).await?;
        }
        Ok(guard)
    }
    pub async fn begin(&self) -> Result<Self, DbErr> {
        let name = format!(
            "appstruct_{}",
            self.shared.sequence.fetch_add(1, Ordering::Relaxed)
        );
        self.execute_unprepared(&format!("SAVEPOINT {name}"))
            .await?;
        Ok(Self {
            shared: self.shared.clone(),
            savepoint: Some(name),
        })
    }
    pub async fn commit(mut self) -> Result<(), DbErr> {
        if let Some(name) = self.savepoint.as_ref() {
            self.execute_unprepared(&format!("RELEASE SAVEPOINT {name}"))
                .await?;
            self.savepoint = None;
            return Ok(());
        }
        self.connection()
            .await?
            .take()
            .ok_or_else(|| DbErr::Protocol("transaction is closed".to_owned()))?
            .commit()
            .await
    }
    pub async fn rollback(mut self) -> Result<(), DbErr> {
        if let Some(name) = self.savepoint.as_ref() {
            self.execute_unprepared(&format!(
                "ROLLBACK TO SAVEPOINT {name}; RELEASE SAVEPOINT {name}"
            ))
            .await?;
            self.savepoint = None;
            return Ok(());
        }
        self.connection()
            .await?
            .take()
            .ok_or_else(|| DbErr::Protocol("transaction is closed".to_owned()))?
            .rollback()
            .await
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if let Some(name) = self.savepoint.take() {
            self.shared
                .pending
                .lock()
                .expect("savepoint queue poisoned")
                .push(format!(
                    "ROLLBACK TO SAVEPOINT {name}; RELEASE SAVEPOINT {name}"
                ));
        }
        // SQLx queues rollback when the last owner of the root transaction drops.
    }
}
#[async_trait]
impl DatabaseExecutor for Transaction {
    async fn execute(&self, sql: Sql) -> Result<PgQueryResult, DbErr> {
        sql.query()?
            .execute(
                &mut **self
                    .connection()
                    .await?
                    .as_mut()
                    .ok_or(DbErr::RowNotFound)?,
            )
            .await
    }
    async fn fetch_optional(&self, sql: Sql) -> Result<Option<PgRow>, DbErr> {
        sql.query()?
            .fetch_optional(
                &mut **self
                    .connection()
                    .await?
                    .as_mut()
                    .ok_or(DbErr::RowNotFound)?,
            )
            .await
    }
    async fn fetch_all(&self, sql: Sql) -> Result<Vec<PgRow>, DbErr> {
        sql.query()?
            .fetch_all(
                &mut **self
                    .connection()
                    .await?
                    .as_mut()
                    .ok_or(DbErr::RowNotFound)?,
            )
            .await
    }
    async fn execute_unprepared(&self, sql: &str) -> Result<PgQueryResult, DbErr> {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(
                &mut **self
                    .connection()
                    .await?
                    .as_mut()
                    .ok_or(DbErr::RowNotFound)?,
            )
            .await
    }
}

pub async fn fetch_model<M: for<'r> sqlx::FromRow<'r, PgRow>>(
    db: &impl DatabaseExecutor,
    sql: Sql,
) -> Result<M, DbErr> {
    let row = db.fetch_optional(sql).await?.ok_or(DbErr::RowNotFound)?;
    M::from_row(&row)
}

/// Carries the result type alongside a PostgreSQL SELECT statement.
#[derive(Clone)]
pub struct Select<M> {
    query: sea_query::SelectStatement,
    model: std::marker::PhantomData<M>,
}
impl<M> Select<M> {
    pub fn new(table: &str) -> Self {
        Self {
            query: sea_query::Query::select()
                .from(sea_query::Alias::new(table))
                .column((sea_query::Alias::new(table), sea_query::Asterisk))
                .to_owned(),
            model: std::marker::PhantomData,
        }
    }
    pub fn filter(mut self, condition: impl sea_query::IntoCondition) -> Self {
        self.query.cond_where(condition);
        self
    }
    pub fn order_by_asc(mut self, column: impl Into<sea_query::SimpleExpr>) -> Self {
        self.query
            .order_by_expr(column.into(), sea_query::Order::Asc);
        self
    }
    pub fn order_by_desc(mut self, column: impl Into<sea_query::SimpleExpr>) -> Self {
        self.query
            .order_by_expr(column.into(), sea_query::Order::Desc);
        self
    }
    pub fn select_only(mut self) -> Self {
        self.query.clear_selects();
        self
    }
    pub fn column(mut self, column: impl Into<sea_query::SimpleExpr>) -> Self {
        self.query.expr(column.into());
        self
    }
    pub fn column_as(mut self, column: impl Into<sea_query::SimpleExpr>, alias: &str) -> Self {
        self.query
            .expr_as(column.into(), sea_query::Alias::new(alias));
        self
    }
    pub fn group_by(mut self, column: impl Into<sea_query::SimpleExpr>) -> Self {
        self.query.add_group_by([column.into()]);
        self
    }
    pub fn limit(mut self, limit: u64) -> Self {
        self.query.limit(limit);
        self
    }
    pub fn offset(mut self, offset: u64) -> Self {
        self.query.offset(offset);
        self
    }
    pub fn lock_exclusive(mut self) -> Self {
        self.query.lock(sea_query::LockType::Update);
        self
    }
    pub fn lock_shared(mut self) -> Self {
        self.query.lock(sea_query::LockType::Share);
        self
    }
    pub fn join(mut self, table: &str, condition: sea_query::SimpleExpr) -> Self {
        self.query.join(
            sea_query::JoinType::InnerJoin,
            sea_query::Alias::new(table),
            condition,
        );
        self
    }
    pub fn into_query(self) -> sea_query::SelectStatement {
        self.query
    }
    pub fn statement(&self) -> Sql {
        let (sql, values) = self.query.build(sea_query::PostgresQueryBuilder);
        Sql::bind(sql, values.0)
    }
    pub async fn count(&self, db: &impl DatabaseExecutor) -> Result<u64, DbErr> {
        let mut query = self.query.clone();
        query.clear_selects().clear_order_by().expr_as(
            sea_query::Expr::cust("count(*)"),
            sea_query::Alias::new("count"),
        );
        let (sql, values) = query.build(sea_query::PostgresQueryBuilder);
        let count: i64 = db
            .fetch_optional(Sql::bind(sql, values.0))
            .await?
            .ok_or(DbErr::RowNotFound)?
            .try_get("count")?;
        u64::try_from(count).map_err(|e| DbErr::Protocol(e.to_string()))
    }
    pub fn into_json(self) -> JsonSelect {
        let (sql, values) = self.query.build(sea_query::PostgresQueryBuilder);
        JsonSelect(Sql::bind(
            format!("SELECT row_to_json(appstruct_row) AS data FROM ({sql}) AS appstruct_row"),
            values.0,
        ))
    }
}
impl<M: for<'r> sqlx::FromRow<'r, PgRow>> Select<M> {
    pub async fn one(self, db: &impl DatabaseExecutor) -> Result<Option<M>, DbErr> {
        db.fetch_optional(self.limit(1).statement())
            .await?
            .as_ref()
            .map(M::from_row)
            .transpose()
    }
    pub async fn all(self, db: &impl DatabaseExecutor) -> Result<Vec<M>, DbErr> {
        db.fetch_all(self.statement())
            .await?
            .iter()
            .map(M::from_row)
            .collect()
    }
}
pub struct JsonSelect(Sql);
impl JsonSelect {
    pub async fn all(self, db: &impl DatabaseExecutor) -> Result<Vec<serde_json::Value>, DbErr> {
        db.fetch_all(self.0)
            .await?
            .iter()
            .map(|row| row.try_get("data"))
            .collect()
    }
}

/// Validate AppStruct's typed filters with sift-query, then embed its predicate
/// in the larger statement containing relation joins and authorization scopes.
pub fn sift_condition<E, T, K>(
    field: sift_query::FieldRef<E, T, K>,
    value: T,
    operator: sift_query::Operator,
) -> Result<sea_query::SimpleExpr, crate::ApiError>
where
    E: sift_query::SearchEntity,
    T: sift_query::IntoScalarValue,
{
    let policy = sift_query::QueryPolicy::default();
    let builder = sift_query::QueryBuilder::<E>::new(&policy);
    let builder = match operator {
        sift_query::Operator::Eq => builder.eq(field, value),
        sift_query::Operator::Ge => builder.ge(field, value),
        sift_query::Operator::Le => builder.le(field, value),
        _ => {
            return Err(crate::ApiError::InvalidQuery(
                "unsupported filter operator".to_owned(),
            ));
        }
    };
    let query = builder
        .build()
        .map_err(|error| crate::ApiError::InvalidQuery(error.to_string()))?;
    let sift_query::PredicateNode::Condition(condition) = &query.predicate().nodes()[0] else {
        return Err(crate::ApiError::Internal);
    };
    let meta = E::META
        .field(condition.field)
        .ok_or(crate::ApiError::Internal)?;
    let column = sea_query::Expr::col((
        sea_query::Alias::new(E::META.from),
        sea_query::Alias::new(meta.column),
    ));
    let value = scalar_value(condition.values[0].clone())?;
    use sea_query::ExprTrait as _;
    Ok(match operator {
        sift_query::Operator::Eq => column.eq(value),
        sift_query::Operator::Ge => column.gte(value),
        sift_query::Operator::Le => column.lte(value),
        _ => return Err(crate::ApiError::Internal),
    })
}
fn scalar_value(value: sift_query::ScalarValue) -> Result<Value, crate::ApiError> {
    use sift_query::ScalarValue as S;
    Ok(match value {
        S::Bool(v) => v.into(),
        S::I32(v) => v.into(),
        S::I64(v) => v.into(),
        S::F32(v) => v.into(),
        S::F64(v) => v.into(),
        S::Decimal(v) => v.into(),
        S::Text(v) => v.into(),
        S::Bytes(v) => v.into(),
        S::Date(v) => v.into(),
        S::Time(v) => v.into(),
        S::DateTime(v) => v.into(),
        S::Timestamp(v) => v.into(),
        S::Uuid(v) => v.into(),
        S::Json(v) => (*v).into(),
        _ => return Err(crate::ApiError::Internal),
    })
}
