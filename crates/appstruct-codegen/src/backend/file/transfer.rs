use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source() -> TokenStream {
    quote! {
        pub(crate) struct PreparedFile {
            object_key: String,
            original_name: String,
            content_type: String,
            size: u64,
            checksum: String,
        }

        impl FileState {
            pub(crate) async fn prepare_owned(
                &self, object_key: &str, original_name: &str, content_type: &str,
                content: axum::body::Bytes,
            ) -> Result<PreparedFile, FileError> {
                self.validate_upload(original_name, content_type, content.len() as u64)?;
                validate_key(object_key)?;
                validate_content(content_type, &content, &self.allowed_content_types)?;
                let prepared = PreparedFile {
                    object_key: object_key.to_owned(), original_name: original_name.to_owned(),
                    content_type: content_type.to_owned(), size: content.len() as u64,
                    checksum: appstruct_runtime::sha256_hex(&content),
                };
                self.provider.put_owned(object_key, content).await?;
                Ok(prepared)
            }

            pub(crate) async fn publish_prepared<C: DatabaseExecutor>(
                &self, database: &C, prepared: &PreparedFile, tenant: Option<uuid::Uuid>,
            ) -> Result<FileMetadata, FileError> {
                insert_metadata(database, &prepared.object_key, &prepared.original_name,
                    &prepared.content_type, prepared.size, &prepared.checksum, tenant).await
            }

            pub(crate) async fn metadata(&self, key: &str, tenant: Option<uuid::Uuid>) -> Result<FileMetadata, FileError> {
                validate_key(key)?;
                load_metadata(&self.database, key, tenant).await
            }

            pub async fn stream(&self, key: &str, tenant: Option<uuid::Uuid>)
                -> Result<(FileMetadata, BoxStream<'static, Result<axum::body::Bytes, FileError>>), FileError>
            {
                use sha2::{Digest as _, Sha256};
                let metadata = self.metadata(key, tenant).await?;
                let mut source = self.provider.get_stream(key).await?;
                let expected_size = metadata.size;
                let expected_checksum = metadata.checksum.clone();
                let stream = async_stream::try_stream! {
                    let mut checksum = Sha256::new();
                    let mut size = 0_u64;
                    let mut pending = None;
                    while let Some(chunk) = source.next().await {
                        let chunk = chunk?;
                        if chunk.is_empty() { continue; }
                        size = size.checked_add(chunk.len() as u64)
                            .filter(|size| *size <= expected_size)
                            .ok_or_else(|| FileError::Storage("stored object size does not match metadata".to_owned()))?;
                        checksum.update(&chunk);
                        if let Some(previous) = pending.replace(chunk) { yield previous; }
                    }
                    let checksum = checksum.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
                    if size != expected_size || checksum != expected_checksum {
                        Err(FileError::Storage("stored object checksum does not match metadata".to_owned()))?;
                    }
                    if let Some(last) = pending { yield last; }
                };
                Ok((metadata, stream.boxed()))
            }
        }
    }
}
