use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Copy)]
struct CachedDigest {
    stamp: FileStamp,
    digest: [u8; 32],
}

static DIGESTS: OnceLock<Mutex<BTreeMap<PathBuf, CachedDigest>>> = OnceLock::new();

pub(crate) fn digest(path: &Path) -> io::Result<[u8; 32]> {
    let metadata = fs::metadata(path)?;
    let stamp = FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    };
    let cache = DIGESTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(cached) = cache
        .lock()
        .map_err(|_| io::Error::other("file fingerprint cache lock was poisoned"))?
        .get(path)
        .copied()
        && cached.stamp == stamp
    {
        return Ok(cached.digest);
    }

    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize().into();
    cache
        .lock()
        .map_err(|_| io::Error::other("file fingerprint cache lock was poisoned"))?
        .insert(path.to_owned(), CachedDigest { stamp, digest });
    Ok(digest)
}
