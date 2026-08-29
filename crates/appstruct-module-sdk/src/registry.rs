use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt};

/// Signed registry response. The signature covers the decoded `payload` bytes exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEnvelope {
    pub schema_version: u32,
    pub payload: String,
    pub sha256: String,
    pub signature: String,
}

/// JSON payload stored in a signed registry envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryPackage {
    pub name: String,
    pub version: String,
    pub appstruct_version: String,
    pub module_api_version: u32,
    pub manifest: String,
    #[serde(default)]
    pub artifacts: Vec<RegistryArtifact>,
}

/// Base64-encoded source file carried by a registry package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryArtifact {
    pub source: String,
    pub content: String,
    pub sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryVerificationError(String);

impl fmt::Display for RegistryVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for RegistryVerificationError {}

/// Verify the package digest and Ed25519 signature, then decode its JSON payload.
///
/// # Errors
///
/// Returns an error for malformed encodings, digest mismatch, invalid signatures, or payload JSON.
pub fn verify_registry_envelope(
    envelope: &RegistryEnvelope,
    public_key: &str,
) -> Result<(RegistryPackage, Vec<u8>), RegistryVerificationError> {
    if envelope.schema_version != 1 {
        return Err(invalid(format!(
            "unsupported registry envelope version {}",
            envelope.schema_version
        )));
    }
    let payload = STANDARD
        .decode(&envelope.payload)
        .map_err(|error| invalid(format!("invalid payload base64: {error}")))?;
    let digest = format!("sha256:{:x}", Sha256::digest(&payload));
    if envelope.sha256 != digest {
        return Err(invalid("registry payload SHA-256 does not match"));
    }
    let key = STANDARD
        .decode(public_key)
        .map_err(|error| invalid(format!("invalid public key base64: {error}")))?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| invalid("Ed25519 public key must contain 32 bytes"))?;
    let key = VerifyingKey::from_bytes(&key)
        .map_err(|error| invalid(format!("invalid Ed25519 public key: {error}")))?;
    let signature = STANDARD
        .decode(&envelope.signature)
        .map_err(|error| invalid(format!("invalid signature base64: {error}")))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|error| invalid(format!("invalid Ed25519 signature: {error}")))?;
    key.verify_strict(&payload, &signature)
        .map_err(|error| invalid(format!("registry signature verification failed: {error}")))?;
    let package = serde_json::from_slice(&payload)
        .map_err(|error| invalid(format!("invalid registry package JSON: {error}")))?;
    Ok((package, payload))
}

fn invalid(message: impl Into<String>) -> RegistryVerificationError {
    RegistryVerificationError(message.into())
}
