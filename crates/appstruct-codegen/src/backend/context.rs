use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn contract() -> TokenStream {
    let methods = methods();
    let connection = connection_trait();
    quote! {
        #[derive(Clone, Copy)]
        enum RequestDatabase<'db> {
            Connection(&'db DatabaseConnection),
            Transaction(&'db DatabaseTransaction),
        }

        #[derive(Clone)]
        pub struct RequestContext<'db> {
            database: RequestDatabase<'db>,
            mail: &'db crate::MailState,
            file: Option<&'db crate::FileState>,
            realtime: Option<&'db crate::RealtimeState>,
            actor: Option<Actor>,
            tenant: Option<TenantId>,
        }

        #methods
        #connection
    }
}

#[allow(clippy::too_many_lines)]
fn methods() -> TokenStream {
    quote! {
        impl<'db> RequestContext<'db> {
            pub fn connection(
                database: &'db DatabaseConnection,
                mail: &'db crate::MailState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Connection(database), mail, file: None, realtime: None, actor, tenant }
            }

            pub fn transaction(
                database: &'db DatabaseTransaction,
                mail: &'db crate::MailState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Transaction(database), mail, file: None, realtime: None, actor, tenant }
            }

            pub fn connection_with_file(
                database: &'db DatabaseConnection,
                mail: &'db crate::MailState,
                file: &'db crate::FileState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Connection(database), mail, file: Some(file), realtime: None, actor, tenant }
            }

            pub(crate) fn connection_with_services(
                database: &'db DatabaseConnection,
                mail: &'db crate::MailState,
                file: &'db crate::FileState,
                realtime: &'db crate::RealtimeState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self {
                    database: RequestDatabase::Connection(database), mail, file: Some(file),
                    realtime: Some(realtime), actor, tenant,
                }
            }

            pub(crate) fn transaction_with_file(
                database: &'db DatabaseTransaction,
                mail: &'db crate::MailState,
                file: &'db crate::FileState,
                realtime: &'db crate::RealtimeState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self {
                    database: RequestDatabase::Transaction(database), mail, file: Some(file),
                    realtime: Some(realtime), actor, tenant,
                }
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
            pub async fn enqueue_job<T: serde::Serialize>(
                &self,
                queue: &str,
                kind: &str,
                payload: &T,
                idempotency_key: Option<&str>,
                run_at: Option<chrono::DateTime<chrono::Utc>>,
            ) -> Result<crate::JobReceipt, crate::JobError> {
                crate::jobs::enqueue(
                    self, queue, kind, payload, idempotency_key, run_at, self.tenant,
                ).await
            }
            pub async fn publish_webhook<T: serde::Serialize>(
                &self, event: &str, payload: &T, idempotency_key: Option<&str>,
            ) -> Result<crate::WebhookReceipt, crate::WebhookError> {
                crate::webhooks::publish(
                    self, event, payload, idempotency_key, self.tenant,
                ).await
            }
            pub fn publish_realtime<T: serde::Serialize>(
                &self, event: &str, payload: &T,
            ) -> Result<crate::RealtimeEvent, serde_json::Error> {
                if let Some(realtime) = self.realtime {
                    realtime.publish(
                        event, payload, self.actor.as_ref().map(|actor| actor.id), self.tenant,
                    )
                } else {
                    crate::RealtimeState::default().publish(
                        event, payload, self.actor.as_ref().map(|actor| actor.id), self.tenant,
                    )
                }
            }
            pub async fn put_file(
                &self, object_key: &str, original_name: &str,
                content_type: &str, content: &[u8],
            ) -> Result<crate::FileMetadata, crate::FileError> {
                self.file.ok_or(crate::FileError::Disabled)?
                    .put(object_key, original_name, content_type, content, self.tenant).await
            }
            pub async fn get_file(
                &self, object_key: &str,
            ) -> Result<(crate::FileMetadata, Vec<u8>), crate::FileError> {
                self.file.ok_or(crate::FileError::Disabled)?.get(object_key, self.tenant).await
            }
            pub async fn delete_file(&self, object_key: &str) -> Result<(), crate::FileError> {
                self.file.ok_or(crate::FileError::Disabled)?.delete(object_key, self.tenant).await
            }
        }
    }
}

fn connection_trait() -> TokenStream {
    quote! {
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
