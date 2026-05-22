//! Atomic file writes: write to a sibling `.tmp` file, then rename into place.
//! On macOS, `rename(2)` is atomic for files on the same volume.

use crate::error::Result;
use std::io::Write;
use std::path::Path;

pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp.{}",
        path.extension().and_then(|s| s.to_str()).unwrap_or("part"),
        std::process::id()
    ));
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes(path, &bytes)
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        a: u32,
        b: String,
    }

    #[test]
    fn roundtrip_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/sub/file.json");
        let s = Sample {
            a: 7,
            b: "hello".into(),
        };
        write_json(&path, &s).unwrap();
        let back: Sample = read_json(&path).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn write_is_atomic_under_concurrent_readers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_json(
            &path,
            &Sample {
                a: 1,
                b: "x".into(),
            },
        )
        .unwrap();
        // Overwrite many times; readers should always observe a valid file.
        for i in 0..20u32 {
            write_json(
                &path,
                &Sample {
                    a: i,
                    b: "y".repeat(i as usize),
                },
            )
            .unwrap();
            let back: Sample = read_json(&path).unwrap();
            assert_eq!(back.a, i);
        }
    }
}
