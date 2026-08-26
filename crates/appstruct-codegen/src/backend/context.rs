use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn contract() -> TokenStream {
    quote! {
        pub type TenantId = uuid::Uuid;

        #[derive(Clone, Copy)]
        enum RequestDatabase<'db> {
            Connection(&'db DatabaseConnection),
            Transaction(&'db DatabaseTransaction),
        }

        #[derive(Clone)]
        pub struct RequestContext<'db> {
            database: RequestDatabase<'db>,
            mail: &'db crate::MailState,
            actor: Option<Actor>,
            tenant: Option<TenantId>,
        }

        impl<'db> RequestContext<'db> {
            pub(crate) fn connection(
                database: &'db DatabaseConnection,
                mail: &'db crate::MailState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Connection(database), mail, actor, tenant }
            }

            pub(crate) fn transaction(
                database: &'db DatabaseTransaction,
                mail: &'db crate::MailState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Transaction(database), mail, actor, tenant }
            }

            pub fn database(&self) -> &Self { self }
            pub fn actor(&self) -> Option<&Actor> { self.actor.as_ref() }
            pub fn tenant(&self) -> Option<TenantId> { self.tenant }
            pub fn require_tenant(&self) -> Result<TenantId, ApiError> {
                self.tenant.ok_or(ApiError::InvalidTenant)
            }
            pub async fn send_mail(
                &self,
                template: &str,
                recipient: &str,
                variables: &std::collections::BTreeMap<String, String>,
            ) -> Result<crate::MailDelivery, crate::MailError> {
                self.mail.send_template(template, recipient, variables, self.tenant).await
            }
        }

        #[async_trait]
        impl ConnectionTrait for RequestContext<'_> {
            fn get_database_backend(&self) -> DbBackend {
                match self.database {
                    RequestDatabase::Connection(database) => database.get_database_backend(),
                    RequestDatabase::Transaction(database) => database.get_database_backend(),
                }
            }

            async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.execute_raw(statement).await,
                    RequestDatabase::Transaction(database) => database.execute_raw(statement).await,
                }
            }

            async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.execute_unprepared(sql).await,
                    RequestDatabase::Transaction(database) => database.execute_unprepared(sql).await,
                }
            }

            async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.query_one_raw(statement).await,
                    RequestDatabase::Transaction(database) => database.query_one_raw(statement).await,
                }
            }

            async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.query_all_raw(statement).await,
                    RequestDatabase::Transaction(database) => database.query_all_raw(statement).await,
                }
            }
        }
    }
}
