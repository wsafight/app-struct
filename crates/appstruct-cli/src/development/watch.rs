use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceFingerprint([u8; 32]);

impl SourceFingerprint {
    pub(super) fn read(project: &Path) -> io::Result<Self> {
        let mut files = Vec::new();
        for candidate in [
            project.join("appstruct.yaml"),
            project.join("appstruct.lock"),
            project.join(".env"),
            project.join(".npmrc"),
            project.join("pnpm-workspace.yaml"),
            project.join("rust-toolchain.toml"),
            project.join("rust-toolchain"),
        ] {
            if candidate.is_file() {
                files.push(candidate);
            }
        }
        for directory in [
            project.join("spec"),
            project.join("modules"),
            project.join("app/backend"),
            project.join("app/web"),
            project.join(".cargo"),
        ] {
            collect_files(&directory, &mut files)?;
        }
        files.sort();
        let mut hash = Sha256::new();
        for file in files {
            let relative = file.strip_prefix(project).map_err(io::Error::other)?;
            hash.update(relative.to_string_lossy().as_bytes());
            hash.update([0]);
            let metadata = fs::symlink_metadata(&file)?;
            if metadata.file_type().is_symlink() {
                hash.update(b"symlink\0");
                hash.update(fs::read_link(file)?.as_os_str().as_encoded_bytes());
            } else {
                hash.update(fs::read(file)?);
            }
            hash.update([0]);
        }
        Ok(Self(hash.finalize().into()))
    }
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(&path, files)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_fingerprint_tracks_project_inputs_only() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join("appstruct.yaml"), "version: 1\n").unwrap();
        let first = SourceFingerprint::read(temporary.path()).unwrap();
        fs::create_dir(temporary.path().join("generated")).unwrap();
        fs::write(temporary.path().join("generated/output"), "ignored").unwrap();
        assert_eq!(first, SourceFingerprint::read(temporary.path()).unwrap());
        fs::create_dir(temporary.path().join("spec")).unwrap();
        fs::write(temporary.path().join("spec/main.yaml"), "domain: main\n").unwrap();
        assert_ne!(first, SourceFingerprint::read(temporary.path()).unwrap());

        let before_environment = SourceFingerprint::read(temporary.path()).unwrap();
        fs::write(
            temporary.path().join(".env"),
            "APPSTRUCT_BIND=127.0.0.1:3001\n",
        )
        .unwrap();
        assert_ne!(
            before_environment,
            SourceFingerprint::read(temporary.path()).unwrap()
        );

        let before_module = SourceFingerprint::read(temporary.path()).unwrap();
        fs::create_dir(temporary.path().join("modules")).unwrap();
        fs::write(
            temporary.path().join("modules/example.toml"),
            "name = 'one'\n",
        )
        .unwrap();
        assert_ne!(
            before_module,
            SourceFingerprint::read(temporary.path()).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn source_fingerprint_does_not_follow_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("modules")).unwrap();
        symlink(temporary.path(), temporary.path().join("modules/loop")).unwrap();
        assert!(SourceFingerprint::read(temporary.path()).is_ok());
    }
}
