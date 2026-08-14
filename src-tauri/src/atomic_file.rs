use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Atomically replaces `path` with `bytes` using a temporary file in the same directory.
///
/// AUD-020: 配置写盘必须原子且失败可观测。临时文件 -> flush -> sync_all -> rename，
/// 失败时清理临时文件并返回错误，绝不留下半写入的配置。
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;

    let mut last_error = None;
    for _ in 0..32 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(
            ".{}.{}.{}.tmp",
            name.to_string_lossy(),
            std::process::id(),
            id
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(mut file) => {
                let result = (|| {
                    file.write_all(bytes)?;
                    file.flush()?;
                    file.sync_all()?;
                    drop(file);
                    fs::rename(&temp, path)?;
                    if let Ok(dir) = fs::File::open(parent) {
                        let _ = dir.sync_all();
                    }
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temp);
                }
                return result;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "unable to create atomic-write temporary file",
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "keym_atomic_test_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn writes_content_atomically_and_reads_back() {
        let dir = temp_dir("basic");
        let path = dir.join("settings.json");
        write(&path, b"{\"enabled\":true}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"enabled\":true}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn overwrites_existing_file_without_partial_state() {
        let dir = temp_dir("overwrite");
        let path = dir.join("settings.json");
        write(&path, b"old").unwrap();
        write(&path, b"new-content").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new-content");
        // 不得残留临时文件
        assert!(std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn creates_parent_directory_if_missing() {
        let dir = temp_dir("parents");
        let path = dir.join("nested/deep/settings.json");
        write(&path, b"{}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
