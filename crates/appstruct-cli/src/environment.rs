use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectEnvironment {
    file_values: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CacheEnvironment {
    Rust,
    Node,
}

impl ProjectEnvironment {
    pub(crate) fn load(project: &Path) -> io::Result<Self> {
        let path = project.join(".env");
        if !path.exists() {
            return Ok(Self::default());
        }
        let values = dotenvy::from_path_iter(&path)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("cannot parse `{}`: {error}", path.display()),
                )
            })?
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("cannot parse `{}`: {error}", path.display()),
                )
            })?;
        Ok(Self {
            file_values: values,
        })
    }

    pub(crate) fn get(&self, name: &str) -> Option<String> {
        env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                self.file_values
                    .get(name)
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            })
    }

    pub(crate) fn apply(&self, command: &mut Command) {
        command.envs(
            self.file_values
                .iter()
                .filter(|(name, _)| env::var_os(name).is_none()),
        );
    }

    pub(crate) fn cache_fingerprint(&self, scope: CacheEnvironment) -> String {
        let mut names = env::vars().map(|(name, _)| name).collect::<BTreeSet<_>>();
        names.extend(self.file_values.keys().cloned());
        let mut hasher = Sha256::new();
        hasher.update(match scope {
            CacheEnvironment::Rust => b"rust".as_slice(),
            CacheEnvironment::Node => b"node".as_slice(),
        });
        for name in names.into_iter().filter(|name| relevant(scope, name)) {
            if let Some(value) = self.get(&name) {
                hasher.update(name.as_bytes());
                hasher.update([0]);
                hasher.update(value.as_bytes());
                hasher.update([0]);
            }
        }
        format!("sha256:{:x}", hasher.finalize())
    }
}

fn relevant(scope: CacheEnvironment, name: &str) -> bool {
    if name == "PATH" || name == "CI" {
        return true;
    }
    match scope {
        CacheEnvironment::Rust => {
            name.starts_with("CARGO_")
                || name.starts_with("RUST")
                || name.starts_with("CC_")
                || name.starts_with("CXX_")
                || name.starts_with("AR_")
                || name.starts_with("CFLAGS")
                || name.starts_with("CXXFLAGS")
                || name.starts_with("LDFLAGS")
                || name.starts_with("PKG_CONFIG")
                || matches!(name, "CC" | "CXX" | "AR")
        }
        CacheEnvironment::Node => {
            name.starts_with("PNPM_")
                || name.starts_with("NPM_CONFIG_")
                || name.starts_with("NODE_")
                || name.starts_with("COREPACK_")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loads_project_environment_without_mutating_the_process() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(
            temporary.path().join(".env"),
            "APPSTRUCT_TEST_VALUE=from-file\n",
        )
        .unwrap();
        let environment = ProjectEnvironment::load(temporary.path()).unwrap();
        assert_eq!(
            environment.get("APPSTRUCT_TEST_VALUE").as_deref(),
            Some("from-file")
        );
    }

    #[test]
    fn cache_fingerprints_include_build_inputs_but_not_runtime_secrets() {
        let first = ProjectEnvironment {
            file_values: BTreeMap::from([
                ("RUSTFLAGS".to_owned(), "-Copt-level=1".to_owned()),
                ("DATABASE_URL".to_owned(), "postgres://one".to_owned()),
            ]),
        };
        let mut changed_secret = first.clone();
        changed_secret
            .file_values
            .insert("DATABASE_URL".to_owned(), "postgres://two".to_owned());
        assert_eq!(
            first.cache_fingerprint(CacheEnvironment::Rust),
            changed_secret.cache_fingerprint(CacheEnvironment::Rust)
        );

        let mut changed_build = first.clone();
        changed_build
            .file_values
            .insert("RUSTFLAGS".to_owned(), "-Copt-level=2".to_owned());
        assert_ne!(
            first.cache_fingerprint(CacheEnvironment::Rust),
            changed_build.cache_fingerprint(CacheEnvironment::Rust)
        );

        let node = ProjectEnvironment {
            file_values: BTreeMap::from([("NODE_ENV".to_owned(), "test".to_owned())]),
        };
        assert!(
            node.cache_fingerprint(CacheEnvironment::Node)
                .starts_with("sha256:")
        );
    }

    #[test]
    fn load_rejects_invalid_dotenv_files() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join(".env"), "UNQUOTED SPACE VALUE\n").unwrap();
        assert!(ProjectEnvironment::load(temporary.path()).is_err());
    }

    #[test]
    fn get_ignores_blank_values_and_apply_sets_file_only_variables() {
        let environment = ProjectEnvironment {
            file_values: BTreeMap::from([
                ("APPSTRUCT_EMPTY".to_owned(), "  ".to_owned()),
                ("APPSTRUCT_FILE_ONLY".to_owned(), "from-file".to_owned()),
            ]),
        };
        assert!(environment.get("APPSTRUCT_EMPTY").is_none());
        assert_eq!(
            environment.get("APPSTRUCT_FILE_ONLY").as_deref(),
            Some("from-file")
        );
        let mut command = Command::new("true");
        environment.apply(&mut command);
    }

    #[test]
    fn relevant_names_match_rust_and_node_scopes() {
        assert!(relevant(CacheEnvironment::Rust, "PATH"));
        assert!(relevant(CacheEnvironment::Rust, "CARGO_HOME"));
        assert!(relevant(CacheEnvironment::Rust, "RUSTFLAGS"));
        assert!(relevant(CacheEnvironment::Rust, "CC"));
        assert!(!relevant(CacheEnvironment::Rust, "DATABASE_URL"));
        assert!(relevant(CacheEnvironment::Node, "PNPM_HOME"));
        assert!(relevant(CacheEnvironment::Node, "NPM_CONFIG_REGISTRY"));
        assert!(relevant(CacheEnvironment::Node, "NODE_ENV"));
        assert!(!relevant(CacheEnvironment::Node, "DATABASE_URL"));
    }
}
