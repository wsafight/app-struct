use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// A generated file planned in memory before any filesystem write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub relative_path: PathBuf,
    pub content: Vec<u8>,
    pub executable: bool,
    pub kind: ArtifactKind,
}

impl Artifact {
    pub(crate) fn text(
        relative_path: impl Into<PathBuf>,
        content: impl Into<String>,
        kind: ArtifactKind,
    ) -> Self {
        Self {
            relative_path: relative_path.into(),
            content: content.into().into_bytes(),
            executable: false,
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    CanonicalIr,
    DatabaseSchema,
    Migration,
    RustManifest,
    RustSource,
    OpenApi,
    TypeScript,
    Web,
}

impl ArtifactKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalIr => "canonical_ir",
            Self::DatabaseSchema => "database_schema",
            Self::Migration => "migration",
            Self::RustManifest => "rust_manifest",
            Self::RustSource => "rust_source",
            Self::OpenApi => "openapi",
            Self::TypeScript => "typescript",
            Self::Web => "web",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodegenError {
    message: String,
}

impl CodegenError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CodegenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl Error for CodegenError {}

impl From<serde_json::Error> for CodegenError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(format!("JSON generation failed: {error}"))
    }
}
