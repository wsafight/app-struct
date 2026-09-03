use super::render;
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::{AppIr, FileProviderIr};
use quote::quote;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.file.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/file.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let (provider_source, provider_init) = provider_source(ir);
    let contract = contract_source();
    let state = state_source(
        &provider_init,
        &ir.file.allowed_content_types,
        ir.file.max_bytes,
    );
    let validation = validation_source();
    render(quote! {
        use async_trait::async_trait;
        use object_store::{
            ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload,
            path::Path as ObjectPath,
        };
        use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
        use sha2::{Digest, Sha256};
        use std::{env, fmt, path::{Component, Path}, sync::Arc};
        #contract
        #state
        #validation
        #provider_source
    })
}

fn contract_source() -> proc_macro2::TokenStream {
    quote! {
        #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
        pub struct FileMetadata {
            pub id: uuid::Uuid,
            pub object_key: String,
            pub original_name: String,
            pub content_type: String,
            pub size: u64,
            pub checksum: String,
            pub tenant_id: Option<uuid::Uuid>,
            pub created_at: chrono::DateTime<chrono::Utc>,
        }
        #[derive(Debug)]
        pub enum FileError {
            Disabled, InvalidKey, InvalidName, InvalidContentType,
            TooLarge { size: u64, max: u64 }, Configuration(String),
            Storage(String), Database(DbErr),
        }
        impl fmt::Display for FileError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Disabled => formatter.write_str("file module is disabled"),
                    Self::InvalidKey => formatter.write_str("file key must be a safe relative path"),
                    Self::InvalidName => formatter.write_str("file name is invalid"),
                    Self::InvalidContentType => formatter.write_str("file content does not match an allowed content type"),
                    Self::TooLarge { size, max } => write!(formatter, "file is too large: {size} bytes exceeds {max}"),
                    Self::Configuration(error) => write!(formatter, "file configuration is invalid: {error}"),
                    Self::Storage(error) => write!(formatter, "file storage operation failed: {error}"),
                    Self::Database(error) => write!(formatter, "file metadata operation failed: {error}"),
                }
            }
        }
        impl std::error::Error for FileError {}
        impl From<DbErr> for FileError { fn from(error: DbErr) -> Self { Self::Database(error) } }
        #[async_trait]
        pub trait FileProvider: Send + Sync {
            fn name(&self) -> &'static str;
            async fn put(&self, object_key: &str, content: &[u8]) -> Result<(), FileError>;
            async fn get(&self, object_key: &str) -> Result<Vec<u8>, FileError>;
            async fn delete(&self, object_key: &str) -> Result<(), FileError>;
        }
    }
}

fn state_source(
    provider_init: &proc_macro2::TokenStream,
    allowed: &[String],
    max_bytes: u64,
) -> proc_macro2::TokenStream {
    let allowed = allowed.iter().map(|value| quote! { #value.to_owned() });
    quote! {
        #[derive(Clone)]
        pub struct FileState {
            database: DatabaseConnection,
            provider: Arc<dyn FileProvider>,
            allowed_content_types: Arc<Vec<String>>,
        }
        impl FileState {
            pub fn from_env(database: DatabaseConnection) -> Result<Self, FileError> {
                let provider = #provider_init;
                Ok(Self {
                    database, provider,
                    allowed_content_types: Arc::new(vec![#(#allowed),*]),
                })
            }
            pub fn with_provider(
                database: DatabaseConnection, provider: Arc<dyn FileProvider>,
                allowed_content_types: Vec<String>,
            ) -> Self {
                Self { database, provider, allowed_content_types: Arc::new(allowed_content_types) }
            }
            pub async fn put(
                &self, object_key: &str, original_name: &str, content_type: &str,
                content: &[u8], tenant_id: Option<uuid::Uuid>,
            ) -> Result<FileMetadata, FileError> {
                validate_key(object_key)?;
                validate_name(original_name)?;
                let size = u64::try_from(content.len()).unwrap_or(u64::MAX);
                if size > #max_bytes { return Err(FileError::TooLarge { size, max: #max_bytes }); }
                validate_content(content_type, content, &self.allowed_content_types)?;
                let checksum = format!("{:x}", Sha256::digest(content));
                self.provider.put(object_key, content).await?;
                match insert_metadata(
                    &self.database, object_key, original_name, content_type, size, &checksum, tenant_id,
                ).await {
                    Ok(metadata) => Ok(metadata),
                    Err(error) => {
                        let _ = self.provider.delete(object_key).await;
                        Err(error)
                    }
                }
            }
            pub async fn get(
                &self, object_key: &str, tenant_id: Option<uuid::Uuid>,
            ) -> Result<(FileMetadata, Vec<u8>), FileError> {
                validate_key(object_key)?;
                let metadata = load_metadata(&self.database, object_key, tenant_id).await?;
                let content = self.provider.get(object_key).await?;
                if format!("{:x}", Sha256::digest(&content)) != metadata.checksum {
                    return Err(FileError::Storage("stored object checksum does not match metadata".to_owned()));
                }
                Ok((metadata, content))
            }
            pub async fn delete(
                &self, object_key: &str, tenant_id: Option<uuid::Uuid>,
            ) -> Result<(), FileError> {
                validate_key(object_key)?;
                load_metadata(&self.database, object_key, tenant_id).await?;
                self.database.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "DELETE FROM \"_appstruct_files\" WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2",
                    [object_key.to_owned().into(), tenant_id.into()],
                )).await?;
                self.provider.delete(object_key).await?;
                Ok(())
            }
        }
        async fn insert_metadata(
            database: &DatabaseConnection, object_key: &str, original_name: &str,
            content_type: &str, size: u64, checksum: &str, tenant_id: Option<uuid::Uuid>,
        ) -> Result<FileMetadata, FileError> {
            let id = uuid::Uuid::now_v7();
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_files\" (id, object_key, original_name, content_type, size, checksum, tenant_id, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP) RETURNING id, object_key, original_name, content_type, size, checksum, tenant_id, created_at",
                [id.into(), object_key.to_owned().into(), original_name.to_owned().into(), content_type.to_owned().into(), i64::try_from(size).unwrap_or(i64::MAX).into(), checksum.to_owned().into(), tenant_id.into()],
            )).await?.ok_or_else(|| DbErr::Custom("file metadata insert returned no row".to_owned()))?;
            row_to_metadata(row).map_err(FileError::from)
        }
        async fn load_metadata(
            database: &DatabaseConnection, object_key: &str, tenant_id: Option<uuid::Uuid>,
        ) -> Result<FileMetadata, FileError> {
            let row = database.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, object_key, original_name, content_type, size, checksum, tenant_id, created_at FROM \"_appstruct_files\" WHERE object_key = $1 AND tenant_id IS NOT DISTINCT FROM $2",
                [object_key.to_owned().into(), tenant_id.into()],
            )).await?.ok_or_else(|| DbErr::RecordNotFound("file metadata not found".to_owned()))?;
            row_to_metadata(row).map_err(FileError::from)
        }
        fn row_to_metadata(row: sea_orm::QueryResult) -> Result<FileMetadata, DbErr> {
            Ok(FileMetadata {
                id: row.try_get("", "id")?, object_key: row.try_get("", "object_key")?,
                original_name: row.try_get("", "original_name")?, content_type: row.try_get("", "content_type")?,
                size: row.try_get::<i64>("", "size")?.try_into().unwrap_or_default(),
                checksum: row.try_get("", "checksum")?, tenant_id: row.try_get("", "tenant_id")?,
                created_at: row.try_get("", "created_at")?,
            })
        }
    }
}

fn validation_source() -> proc_macro2::TokenStream {
    quote! {
        fn validate_key(value: &str) -> Result<(), FileError> {
            if value.is_empty() || value.len() > 512 || value.contains('\\') || value.contains('\0')
                || Path::new(value).is_absolute()
                || value.split('/').any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
                || Path::new(value).components().any(|part| !matches!(part, Component::Normal(_))) {
                return Err(FileError::InvalidKey);
            }
            Ok(())
        }
        fn validate_name(value: &str) -> Result<(), FileError> {
            if value.is_empty() || matches!(value, "." | "..") || value.len() > 255
                || value.contains('/') || value.contains('\\')
                || value.chars().any(char::is_control) {
                Err(FileError::InvalidName)
            } else { Ok(()) }
        }
        fn validate_content(
            content_type: &str, content: &[u8], allowed: &[String],
        ) -> Result<(), FileError> {
            let allowed = allowed.iter().any(|candidate| {
                candidate == content_type || (candidate.ends_with("/*")
                    && content_type.starts_with(&candidate[..candidate.len() - 1]))
            });
            if !allowed { return Err(FileError::InvalidContentType); }
            let valid = if content_type == "application/octet-stream" {
                true
            } else if content_type == "application/json" {
                serde_json::from_slice::<serde_json::Value>(content).is_ok()
            } else if content_type.starts_with("text/") {
                !content.contains(&0) && std::str::from_utf8(content).is_ok()
            } else {
                infer::get(content).is_some_and(|kind| kind.mime_type() == content_type)
            };
            if valid { Ok(()) } else { Err(FileError::InvalidContentType) }
        }
    }
}

fn provider_source(ir: &AppIr) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let source = object_store_provider();
    let init = match ir.file.provider {
        FileProviderIr::Local => {
            let root = &ir.file.local_root;
            quote! {{
                let root = env::var("APPSTRUCT_FILE_ROOT").unwrap_or_else(|_| #root.to_owned());
                std::fs::create_dir_all(&root).map_err(|error| FileError::Configuration(error.to_string()))?;
                let store = object_store::local::LocalFileSystem::new_with_prefix(root)
                    .map_err(|error| FileError::Configuration(error.to_string()))?;
                Arc::new(ObjectStoreProvider { provider_name: "local", store: Arc::new(store) })
                    as Arc<dyn FileProvider>
            }}
        }
        FileProviderIr::S3 => quote! {{
            let required = |name: &str| env::var(name)
                .map_err(|_| FileError::Configuration(format!("{name} is required")));
            let endpoint = required("APPSTRUCT_S3_ENDPOINT")?;
            let allow_http = env::var("APPSTRUCT_S3_ALLOW_HTTP").as_deref() == Ok("true");
            let store = object_store::aws::AmazonS3Builder::new()
                .with_bucket_name(required("APPSTRUCT_S3_BUCKET")?)
                .with_endpoint(endpoint)
                .with_access_key_id(required("APPSTRUCT_S3_ACCESS_KEY")?)
                .with_secret_access_key(required("APPSTRUCT_S3_SECRET_KEY")?)
                .with_region(env::var("APPSTRUCT_S3_REGION").unwrap_or_else(|_| "us-east-1".to_owned()))
                .with_allow_http(allow_http)
                .build().map_err(|error| FileError::Configuration(error.to_string()))?;
            Arc::new(ObjectStoreProvider { provider_name: "s3", store: Arc::new(store) })
                as Arc<dyn FileProvider>
        }},
    };
    (source, init)
}

fn object_store_provider() -> proc_macro2::TokenStream {
    quote! {
        struct ObjectStoreProvider {
            provider_name: &'static str,
            store: Arc<dyn ObjectStore>,
        }
        #[async_trait]
        impl FileProvider for ObjectStoreProvider {
            fn name(&self) -> &'static str { self.provider_name }
            async fn put(&self, object_key: &str, content: &[u8]) -> Result<(), FileError> {
                let options = PutOptions { mode: PutMode::Create, ..PutOptions::default() };
                self.store.put_opts(
                    &ObjectPath::from(object_key), PutPayload::from(content.to_vec()), options,
                )
                    .await.map(|_| ()).map_err(|error| FileError::Storage(error.to_string()))
            }
            async fn get(&self, object_key: &str) -> Result<Vec<u8>, FileError> {
                self.store.get(&ObjectPath::from(object_key)).await
                    .map_err(|error| FileError::Storage(error.to_string()))?.bytes().await
                    .map(|bytes| bytes.to_vec()).map_err(|error| FileError::Storage(error.to_string()))
            }
            async fn delete(&self, object_key: &str) -> Result<(), FileError> {
                match self.store.delete(&ObjectPath::from(object_key)).await {
                    Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                    Err(error) => Err(FileError::Storage(error.to_string())),
                }
            }
        }
    }
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use sea_orm::DatabaseConnection;
        use std::{fmt, sync::Arc};
        #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)] pub struct FileMetadata;
        #[derive(Debug)] pub enum FileError { Disabled }
        impl fmt::Display for FileError { fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result { formatter.write_str("file module is disabled") } }
        impl std::error::Error for FileError {}
        #[async_trait::async_trait] pub trait FileProvider: Send + Sync {}
        #[derive(Clone, Default)] pub struct FileState;
        impl FileState {
            pub fn from_env(_database: DatabaseConnection) -> Result<Self, FileError> { Ok(Self) }
            pub fn with_provider(_database: DatabaseConnection, _provider: Arc<dyn FileProvider>, _allowed_content_types: Vec<String>) -> Self { Self }
            pub async fn put(&self, _key: &str, _name: &str, _content_type: &str, _content: &[u8], _tenant_id: Option<uuid::Uuid>) -> Result<FileMetadata, FileError> { Err(FileError::Disabled) }
            pub async fn get(&self, _key: &str, _tenant_id: Option<uuid::Uuid>) -> Result<(FileMetadata, Vec<u8>), FileError> { Err(FileError::Disabled) }
            pub async fn delete(&self, _key: &str, _tenant_id: Option<uuid::Uuid>) -> Result<(), FileError> { Err(FileError::Disabled) }
        }
    })
}
