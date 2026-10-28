use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn contract() -> TokenStream {
    let methods = methods();
    let connection = connection_trait();
    quote! {
        #[derive(Clone, Copy)]
        enum RequestDatabase<'db> {
            Connection(&'db Database),
            Transaction(&'db Transaction),
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
                database: &'db Database,
                mail: &'db crate::MailState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Connection(database), mail, file: None, realtime: None, actor, tenant }
            }

            pub fn transaction(
                database: &'db Transaction,
                mail: &'db crate::MailState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Transaction(database), mail, file: None, realtime: None, actor, tenant }
            }

            pub fn connection_with_file(
                database: &'db Database,
                mail: &'db crate::MailState,
                file: &'db crate::FileState,
                actor: Option<Actor>,
                tenant: Option<TenantId>,
            ) -> Self {
                Self { database: RequestDatabase::Connection(database), mail, file: Some(file), realtime: None, actor, tenant }
            }

            pub(crate) fn connection_with_services(
                database: &'db Database,
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

            pub fn transaction_with_file(
                database: &'db Transaction,
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
                    .put_with_connection(
                        self, object_key, original_name, content_type, content, self.tenant,
                    ).await
            }
            pub async fn get_file(
                &self, object_key: &str,
            ) -> Result<(crate::FileMetadata, Vec<u8>), crate::FileError> {
                self.file.ok_or(crate::FileError::Disabled)?
                    .get_with_connection(self, object_key, self.tenant).await
            }
            pub async fn retain_file(&self, object_key: &str, entity: &str, record_id: &str) -> Result<(), crate::FileError> {
                self.file.ok_or(crate::FileError::Disabled)?
                    .retain_with_connection(self, object_key, entity, record_id, self.tenant).await
            }
            pub async fn release_file(&self, object_key: &str, entity: &str, record_id: &str) -> Result<(), crate::FileError> {
                self.file.ok_or(crate::FileError::Disabled)?
                    .release_with_connection(self, object_key, entity, record_id, self.tenant).await
            }
            pub async fn delete_file(&self, object_key: &str) -> Result<(), crate::FileError> {
                let file = self.file.ok_or(crate::FileError::Disabled)?;
                match self.database {
                    RequestDatabase::Connection(_) => file.delete(object_key, self.tenant).await,
                    RequestDatabase::Transaction(database) => file.delete_with_connection(database, object_key, self.tenant).await,
                }
            }
        }
    }
}

fn connection_trait() -> TokenStream {
    quote! {
        #[async_trait]
        impl DatabaseExecutor for RequestContext<'_> {
            async fn execute(&self, statement: Sql) -> Result<PgQueryResult, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.execute(statement).await,
                    RequestDatabase::Transaction(database) => database.execute(statement).await,
                }
            }

            async fn execute_unprepared(&self, sql: &str) -> Result<PgQueryResult, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.execute_unprepared(sql).await,
                    RequestDatabase::Transaction(database) => database.execute_unprepared(sql).await,
                }
            }

            async fn fetch_optional(&self, statement: Sql) -> Result<Option<PgRow>, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.fetch_optional(statement).await,
                    RequestDatabase::Transaction(database) => database.fetch_optional(statement).await,
                }
            }

            async fn fetch_all(&self, statement: Sql) -> Result<Vec<PgRow>, DbErr> {
                match self.database {
                    RequestDatabase::Connection(database) => database.fetch_all(statement).await,
                    RequestDatabase::Transaction(database) => database.fetch_all(statement).await,
                }
            }
        }
    }
}
