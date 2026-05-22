//! Source extraction: turn a file on disk into plain text + a `SourceKind`.
//!
//! Phase 1 supports: `.md`, `.markdown`, `.txt`, `.pdf`. Other extensions
//! are explicitly rejected — we'd rather refuse than silently mis-ingest.

use crate::error::{QuillError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Markdown,
    Plain,
    Pdf,
}

#[derive(Debug, Clone)]
pub struct Extracted {
    pub kind: SourceKind,
    pub text: String,
}

pub fn extract_from_path(path: &Path) -> Result<Extracted> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("md" | "markdown") => Ok(Extracted {
            kind: SourceKind::Markdown,
            text: read_text(path)?,
        }),
        Some("txt") => Ok(Extracted {
            kind: SourceKind::Plain,
            text: read_text(path)?,
        }),
        Some("pdf") => Ok(Extracted {
            kind: SourceKind::Pdf,
            text: extract_pdf(path)?,
        }),
        Some(other) => Err(QuillError::InvalidArgument(format!(
            "unsupported file extension: .{other}"
        ))),
        None => Err(QuillError::InvalidArgument(
            "file has no extension; cannot determine type".into(),
        )),
    }
}

fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    // Be lenient: accept BOM, normalize line endings.
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if s.starts_with('\u{FEFF}') {
        s.remove(0);
    }
    if s.contains('\r') {
        s = s.replace("\r\n", "\n").replace('\r', "\n");
    }
    Ok(s)
}

fn extract_pdf(path: &Path) -> Result<String> {
    // pdf-extract returns Err for malformed PDFs and many edge cases.
    // We surface the error verbatim so the user can see which file failed.
    pdf_extract::extract_text(path).map_err(|e| {
        QuillError::Storage(format!("PDF extraction failed for {}: {e}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detects_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        std::fs::write(&p, "# Hello\n\nWorld\n").unwrap();
        let r = extract_from_path(&p).unwrap();
        assert_eq!(r.kind, SourceKind::Markdown);
        assert!(r.text.contains("Hello"));
    }

    #[test]
    fn detects_plain() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.txt");
        std::fs::write(&p, "just words").unwrap();
        let r = extract_from_path(&p).unwrap();
        assert_eq!(r.kind, SourceKind::Plain);
    }

    #[test]
    fn normalizes_crlf_and_strips_bom() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bom.md");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(&[0xEF, 0xBB, 0xBF]).unwrap();
        f.write_all(b"# T\r\n\r\nbody\r\n").unwrap();
        drop(f);
        let r = extract_from_path(&p).unwrap();
        assert!(!r.text.starts_with('\u{FEFF}'));
        assert!(!r.text.contains('\r'));
        assert!(r.text.contains("# T\n\nbody\n"));
    }

    #[test]
    fn rejects_unknown_extension() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nope.docx");
        std::fs::write(&p, "x").unwrap();
        assert!(matches!(
            extract_from_path(&p),
            Err(QuillError::InvalidArgument(_))
        ));
    }
}
