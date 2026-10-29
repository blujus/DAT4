//! File formats: native `.mm.json`, JSON interchange (nested / flat / generic),
//! CSV (adjacency / path / indent), Markdown outline. Pure `&[u8]`/`String` in and
//! out so it works in WASM and the CLI; only `native::save_path`/`load_path` touch disk.
#![forbid(unsafe_code)]

pub mod csv_io;
pub mod json;
pub mod native;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, IoError>;

/// Non-fatal problems found while importing; shown to the user before commit.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub warnings: Vec<String>,
}

impl Report {
    pub fn warn(&mut self, msg: impl Into<String>) {
        if self.warnings.len() < 200 {
            self.warnings.push(msg.into());
        }
    }
}

/// Decode bytes as UTF-8 (with or without BOM) or UTF-16 (BOM required).
pub fn decode_text(bytes: &[u8], report: &mut Report) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], le: bool| -> String {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            report.warn("input is not valid UTF-8; invalid bytes were replaced");
            String::from_utf8_lossy(bytes).into_owned()
        }
    }
}

/// Write a file atomically: temp file in the same directory, then rename.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
