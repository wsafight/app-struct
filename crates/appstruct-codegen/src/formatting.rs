use crate::{Artifact, ArtifactKind, CodegenError, RustfmtTimings};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const FORMAT_CACHE_LIMIT: usize = 4_096;
type FormatCache = HashMap<[u8; 32], Vec<u8>>;

pub(super) fn format_rust_artifacts(
    artifacts: &mut [Artifact],
    persistent_cache: Option<&Path>,
) -> Result<RustfmtTimings, CodegenError> {
    let total_started = Instant::now();
    let rust_indexes = artifacts
        .iter()
        .enumerate()
        .filter_map(|(index, artifact)| {
            (artifact.kind == ArtifactKind::RustSource).then_some(index)
        })
        .collect::<Vec<_>>();
    if rust_indexes.is_empty() {
        return Ok(RustfmtTimings {
            total: total_started.elapsed(),
            ..RustfmtTimings::default()
        });
    }
    let rustfmt_identity = persistent_cache.map(|_| rustfmt_identity()).transpose()?;
    let cache = rust_format_cache();
    let mut cache_keys = HashMap::new();
    let mut misses = Vec::new();
    let mut memory_hits = 0;
    let mut persistent_hits = 0;
    {
        let mut cache = cache
            .lock()
            .map_err(|_| CodegenError::new("Rust format cache lock was poisoned"))?;
        for index in rust_indexes {
            let key = format_cache_key(&artifacts[index], rustfmt_identity.as_deref());
            if let Some(content) = cache.get(&key) {
                artifacts[index].content.clone_from(content);
                memory_hits += 1;
            } else if let Some(content) =
                persistent_cache.and_then(|directory| read_persistent_format(directory, &key))
            {
                artifacts[index].content.clone_from(&content);
                cache.insert(key, content);
                persistent_hits += 1;
            } else {
                cache_keys.insert(index, key);
                misses.push(index);
            }
        }
    }
    if misses.is_empty() {
        return Ok(RustfmtTimings {
            total: total_started.elapsed(),
            memory_hits,
            persistent_hits,
            misses: 0,
            ..RustfmtTimings::default()
        });
    }
    let process_started = Instant::now();
    let formatted = format_rust_chunks(artifacts, &misses)?;
    let process = process_started.elapsed();
    for (index, content) in &formatted {
        artifacts[*index].content.clone_from(content);
    }
    let cached = formatted
        .into_iter()
        .map(|(index, content)| (cache_keys[&index], content))
        .collect::<Vec<_>>();
    {
        let mut cache = cache
            .lock()
            .map_err(|_| CodegenError::new("Rust format cache lock was poisoned"))?;
        if cache.len() + cached.len() > FORMAT_CACHE_LIMIT {
            cache.clear();
        }
        cache.extend(cached.iter().cloned());
    }
    if let Some(directory) = persistent_cache {
        for (key, content) in &cached {
            let _ = write_persistent_format(directory, key, content);
        }
        prune_persistent_formats(directory, FORMAT_CACHE_LIMIT);
    }
    Ok(RustfmtTimings {
        total: total_started.elapsed(),
        process,
        memory_hits,
        persistent_hits,
        misses: misses.len(),
    })
}

fn format_rust_chunks(
    artifacts: &[Artifact],
    indexes: &[usize],
) -> Result<Vec<(usize, Vec<u8>)>, CodegenError> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(indexes.len());
    let chunks = balanced_format_chunks(artifacts, indexes, workers);
    std::thread::scope(|scope| {
        let handles = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || format_rust_chunk(artifacts, &chunk)))
            .collect::<Vec<_>>();
        let mut formatted = Vec::with_capacity(indexes.len());
        for handle in handles {
            let chunk = handle
                .join()
                .map_err(|_| CodegenError::new("Rust formatting worker panicked"))??;
            formatted.extend(chunk);
        }
        Ok(formatted)
    })
}

fn balanced_format_chunks(
    artifacts: &[Artifact],
    indexes: &[usize],
    workers: usize,
) -> Vec<Vec<usize>> {
    let mut ordered = indexes.to_vec();
    ordered.sort_by(|left, right| {
        artifacts[*right]
            .content
            .len()
            .cmp(&artifacts[*left].content.len())
            .then_with(|| left.cmp(right))
    });
    let mut chunks = vec![Vec::new(); workers];
    let mut sizes = vec![0_usize; workers];
    for index in ordered {
        let target = (1..workers).fold(0, |smallest, candidate| {
            if sizes[candidate] < sizes[smallest] {
                candidate
            } else {
                smallest
            }
        });
        sizes[target] = sizes[target].saturating_add(artifacts[index].content.len());
        chunks[target].push(index);
    }
    chunks
}

fn format_rust_chunk(
    artifacts: &[Artifact],
    indexes: &[usize],
) -> Result<Vec<(usize, Vec<u8>)>, CodegenError> {
    let mut input = String::new();
    for index in indexes {
        let source = std::str::from_utf8(&artifacts[*index].content)
            .map_err(|error| CodegenError::new(format!("generated Rust is not UTF-8: {error}")))?;
        input.push_str(&start_marker(*index));
        input.push_str(source);
        if !source.ends_with('\n') {
            input.push('\n');
        }
        input.push_str(&end_marker(*index));
    }
    let output = run_rustfmt(&input)?;
    split_formatted_artifacts(&output, indexes)
}

fn split_formatted_artifacts(
    output: &str,
    indexes: &[usize],
) -> Result<Vec<(usize, Vec<u8>)>, CodegenError> {
    let mut remainder = output;
    let mut formatted = Vec::with_capacity(indexes.len());
    for index in indexes {
        let start = start_marker(*index);
        let end = end_marker(*index);
        let (_, after_start) = remainder
            .split_once(&start)
            .ok_or_else(|| CodegenError::new("rustfmt removed an artifact start marker"))?;
        let (source, after_end) = after_start
            .split_once(&end)
            .ok_or_else(|| CodegenError::new("rustfmt removed an artifact end marker"))?;
        formatted.push((
            *index,
            format!("{}\n", source.trim_matches('\n')).into_bytes(),
        ));
        remainder = after_end;
    }
    Ok(formatted)
}

fn rust_format_cache() -> &'static Mutex<FormatCache> {
    static CACHE: OnceLock<Mutex<FormatCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn format_cache_key(artifact: &Artifact, rustfmt_identity: Option<&str>) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"appstruct-rustfmt-cache-v1\0--edition=2024\0");
    if let Some(identity) = rustfmt_identity {
        hash.update(identity.as_bytes());
    }
    hash.update([0]);
    hash.update(artifact.relative_path.as_os_str().as_encoded_bytes());
    hash.update([0]);
    hash.update(&artifact.content);
    hash.finalize().into()
}

fn rustfmt_identity() -> Result<String, CodegenError> {
    let output = Command::new("rustfmt")
        .arg("--version")
        .output()
        .map_err(|error| CodegenError::new(format!("failed to identify rustfmt: {error}")))?;
    if !output.status.success() {
        return Err(CodegenError::new(format!(
            "failed to identify rustfmt: process exited with {}",
            output.status
        )));
    }
    let identity = String::from_utf8(output.stdout)
        .map_err(|error| CodegenError::new(format!("rustfmt version is not UTF-8: {error}")))?;
    let identity = identity.trim();
    if identity.is_empty() {
        Err(CodegenError::new("rustfmt version output was empty"))
    } else {
        Ok(identity.to_owned())
    }
}

fn read_persistent_format(directory: &Path, key: &[u8; 32]) -> Option<Vec<u8>> {
    fs::read(persistent_format_path(directory, key)).ok()
}

fn write_persistent_format(directory: &Path, key: &[u8; 32], content: &[u8]) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let destination = persistent_format_path(directory, key);
    if destination.is_file() {
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(content)?;
    temporary.flush()?;
    match temporary.persist_noclobber(destination) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error),
    }
}

fn persistent_format_path(directory: &Path, key: &[u8; 32]) -> PathBuf {
    let mut name = String::with_capacity(67);
    for byte in key {
        use std::fmt::Write as _;
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".rs");
    directory.join(name)
}

fn prune_persistent_formats(directory: &Path, limit: usize) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|value| value == "rs"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    if files.len() <= limit {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    let remove = files.len() - limit;
    for (_, path) in files.into_iter().take(remove) {
        let _ = fs::remove_file(path);
    }
}

fn run_rustfmt(source: &str) -> Result<String, CodegenError> {
    let mut child = Command::new("rustfmt")
        .args(["--emit", "stdout", "--edition", "2024"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| CodegenError::new(format!("failed to start rustfmt: {error}")))?;
    child
        .stdin
        .take()
        .ok_or_else(|| CodegenError::new("rustfmt stdin was not available"))?
        .write_all(source.as_bytes())
        .map_err(|error| CodegenError::new(format!("failed to write to rustfmt: {error}")))?;
    let output = child
        .wait_with_output()
        .map_err(|error| CodegenError::new(format!("failed to wait for rustfmt: {error}")))?;
    if !output.status.success() {
        return Err(CodegenError::new(format!(
            "rustfmt failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| CodegenError::new(format!("rustfmt returned invalid UTF-8: {error}")))
}

fn start_marker(index: usize) -> String {
    format!("// __APPSTRUCT_ARTIFACT_{index}_START__\n")
}

fn end_marker(index: usize) -> String {
    format!("// __APPSTRUCT_ARTIFACT_{index}_END__\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustfmt_chunks_are_balanced_by_source_bytes() {
        let artifacts = [100, 80, 20, 10]
            .map(|size| Artifact::text("source.rs", "x".repeat(size), ArtifactKind::RustSource));
        let chunks = balanced_format_chunks(&artifacts, &[0, 1, 2, 3], 2);
        let sizes = chunks
            .iter()
            .map(|chunk| {
                chunk
                    .iter()
                    .map(|index| artifacts[*index].content.len())
                    .sum::<usize>()
            })
            .collect::<Vec<_>>();
        assert_eq!(sizes, [110, 100]);
    }

    #[test]
    fn formatted_artifacts_are_split_with_a_forward_cursor() {
        let output = format!(
            "preamble\n{}\nfn first() {{}}\n{}{}\nfn second() {{}}\n{}",
            start_marker(7),
            end_marker(7),
            start_marker(3),
            end_marker(3),
        );
        let formatted = split_formatted_artifacts(&output, &[7, 3]).unwrap();
        assert_eq!(formatted[0], (7, b"fn first() {}\n".to_vec()));
        assert_eq!(formatted[1], (3, b"fn second() {}\n".to_vec()));
    }

    #[test]
    fn persistent_rustfmt_cache_survives_memory_cache_eviction() {
        let temporary = tempfile::tempdir().unwrap();
        let source = "fn appstruct_persistent_format_probe( ){println!(\"probe\");}";
        let mut first = [Artifact::text(
            "persistent-probe.rs",
            source,
            ArtifactKind::RustSource,
        )];
        let first_timings = format_rust_artifacts(&mut first, Some(temporary.path())).unwrap();
        assert_eq!(first_timings.misses, 1);
        assert_eq!(first_timings.persistent_hits, 0);

        let identity = rustfmt_identity().unwrap();
        let unformatted = Artifact::text("persistent-probe.rs", source, ArtifactKind::RustSource);
        let key = format_cache_key(&unformatted, Some(&identity));
        rust_format_cache().lock().unwrap().remove(&key);
        let mut second = [unformatted];
        let second_timings = format_rust_artifacts(&mut second, Some(temporary.path())).unwrap();

        assert_eq!(second_timings.persistent_hits, 1);
        assert_eq!(second_timings.misses, 0);
        assert_eq!(first[0].content, second[0].content);
    }
}
