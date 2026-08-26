use std::collections::BTreeMap;
use std::env;
use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Default)]
pub(crate) struct ProjectEnvironment {
    file_values: BTreeMap<String, String>,
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
}
