use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

pub(crate) const CACHE_SCHEMA_VERSION: u32 = appstruct_contracts::CACHE_SCHEMA.current;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct CacheKey {
    operation: String,
    inputs_sha256: String,
    tools: BTreeMap<String, String>,
    environment_sha256: String,
    host_os: String,
    host_arch: String,
}

impl CacheKey {
    pub(crate) fn new(operation: &str, inputs_sha256: String) -> Self {
        Self {
            operation: operation.to_owned(),
            inputs_sha256,
            tools: BTreeMap::new(),
            environment_sha256: String::new(),
            host_os: std::env::consts::OS.to_owned(),
            host_arch: std::env::consts::ARCH.to_owned(),
        }
    }

    pub(crate) fn with_tool(mut self, name: &str, identity: String) -> Self {
        self.tools.insert(name.to_owned(), identity);
        self
    }

    pub(crate) fn with_environment(mut self, fingerprint: String) -> Self {
        self.environment_sha256 = fingerprint;
        self
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheEnvelope<T> {
    schema_version: u32,
    key: CacheKey,
    value: T,
}

pub(crate) fn load<T: DeserializeOwned>(path: &Path, expected: &CacheKey) -> io::Result<Option<T>> {
    let source = match std::fs::read(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let Ok(envelope) = serde_json::from_slice::<CacheEnvelope<T>>(&source) else {
        return Ok(None);
    };
    if envelope.schema_version == CACHE_SCHEMA_VERSION && envelope.key == *expected {
        Ok(Some(envelope.value))
    } else {
        Ok(None)
    }
}

pub(crate) fn store<T: Serialize>(path: &Path, key: CacheKey, value: T) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("cache state has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let envelope = CacheEnvelope {
        schema_version: CACHE_SCHEMA_VERSION,
        key,
        value,
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &envelope).map_err(io::Error::other)?;
    temporary.write_all(b"\n")?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn command_identity(command: &mut Command, name: &str) -> io::Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "cannot identify `{name}`: process exited with {}",
            output.status
        )));
    }
    let identity = [output.stdout, output.stderr]
        .into_iter()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if identity.is_empty() {
        Err(io::Error::other(format!(
            "cannot identify `{name}`: version output was empty"
        )))
    } else {
        Ok(identity)
    }
}
