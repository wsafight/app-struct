use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source() -> TokenStream {
    quote! {
        fn sha256_hex(bytes: &[u8]) -> String {
            format!("sha256:{:x}", Sha256::digest(bytes))
        }

        fn snapshot_key() -> Result<aead::LessSafeKey, ApiError> {
            let encoded = std::env::var("APPSTRUCT_REPORT_SNAPSHOT_KEY")
                .map_err(|_| ApiError::ReportConfiguration)?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.trim())
                .map_err(|_| ApiError::ReportConfiguration)?;
            let key = aead::UnboundKey::new(&aead::AES_256_GCM, &bytes)
                .map_err(|_| ApiError::ReportConfiguration)?;
            Ok(aead::LessSafeKey::new(key))
        }

        fn encrypt_snapshot(run_id: uuid::Uuid, plaintext: &[u8]) -> Result<String, ApiError> {
            let key = snapshot_key()?;
            let mut nonce_bytes = [0_u8; 12];
            SystemRandom::new().fill(&mut nonce_bytes)
                .map_err(|_| ApiError::ReportConfiguration)?;
            let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
            let mut sealed = plaintext.to_vec();
            key.seal_in_place_append_tag(
                nonce, aead::Aad::from(run_id.as_bytes().as_slice()), &mut sealed,
            ).map_err(|_| ApiError::ReportConfiguration)?;
            let mut envelope = nonce_bytes.to_vec();
            envelope.extend_from_slice(&sealed);
            Ok(base64::engine::general_purpose::STANDARD.encode(envelope))
        }

        fn decrypt_snapshot(run_id: uuid::Uuid, encoded: &str) -> Result<Vec<u8>, String> {
            let key = snapshot_key().map_err(|_| "REPORT_CONFIGURATION".to_owned())?;
            let envelope = base64::engine::general_purpose::STANDARD.decode(encoded)
                .map_err(|_| "REPORT_SNAPSHOT_INVALID".to_owned())?;
            if envelope.len() < 28 { return Err("REPORT_SNAPSHOT_INVALID".to_owned()); }
            let nonce_bytes: [u8; 12] = envelope[..12].try_into()
                .map_err(|_| "REPORT_SNAPSHOT_INVALID".to_owned())?;
            let mut sealed = envelope[12..].to_vec();
            let plaintext = key.open_in_place(
                aead::Nonce::assume_unique_for_key(nonce_bytes),
                aead::Aad::from(run_id.as_bytes().as_slice()), &mut sealed,
            ).map_err(|_| "REPORT_SNAPSHOT_INVALID".to_owned())?;
            Ok(plaintext.to_vec())
        }
    }
}
