//! `cargo xtask fetch`: download the pinned upstream font files and verify their SHA-256.
//!
//! The pins live in `fonts/upstream.txt` (`file | sha256 | url`). Downloads go through `curl`
//! (so proxies and system certificates work as they do for git), into a temporary file that is
//! moved into place only once its checksum matches.

use std::path::{Path, PathBuf};

/// One line of `fonts/upstream.txt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    /// Path relative to the repository root.
    pub file: String,
    /// Lower-case hex SHA-256 the download must have.
    pub sha256: String,
    /// Commit-pinned download URL.
    pub url: String,
}

/// Parse `fonts/upstream.txt`. Blank lines and `#` comments are skipped.
pub fn parse_pins(text: &str) -> Result<Vec<Pin>, String> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .map(|(n, l)| {
            let fields: Vec<&str> = l.split(" | ").map(str::trim).collect();
            let [file, sha256, url] = fields.as_slice() else {
                return Err(format!("upstream.txt line {n}: want `file | sha256 | url`"));
            };
            let hex = sha256.len() == 64
                && sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !hex {
                return Err(format!("upstream.txt line {n}: bad sha256"));
            }
            if file.is_empty() || file.contains("..") || Path::new(file).is_absolute() {
                return Err(format!("upstream.txt line {n}: bad file path {file:?}"));
            }
            if !(url.starts_with("https://") || url.starts_with("file://")) {
                return Err(format!("upstream.txt line {n}: url must be https://"));
            }
            Ok(Pin {
                file: (*file).to_string(),
                sha256: (*sha256).to_string(),
                url: (*url).to_string(),
            })
        })
        .collect()
}

/// Read and parse `fonts/upstream.txt` of the checkout at `root`.
pub fn read_pins(root: &Path) -> Result<Vec<Pin>, String> {
    let path = root.join("fonts/upstream.txt");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_pins(&text)
}

/// What happened to one pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file was already there with the right checksum.
    Present,
    /// It was downloaded and verified.
    Fetched,
    /// (`verify` only) it is missing or differs.
    Bad(String),
}

/// Downloads `url` to `to` (an implementation detail the tests replace).
pub type Download = dyn Fn(&str, &Path) -> Result<(), String>;

/// Download with `curl`, failing on HTTP errors and retrying transient failures.
pub fn curl(url: &str, to: &Path) -> Result<(), String> {
    let status = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "3",
        ])
        .arg("--output")
        .arg(to)
        .arg(url)
        .status()
        .map_err(|e| format!("running curl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("curl {url}: {status}"))
    }
}

fn matches(path: &Path, sha256: &str) -> bool {
    std::fs::read(path).is_ok_and(|b| crate::sha256_hex(&b) == sha256)
}

/// Fetch every pin into `dest` (same relative paths). Files already present with the right
/// checksum are skipped unless `force`. A download whose checksum differs is an error and is not
/// written.
pub fn fetch(
    pins: &[Pin],
    dest: &Path,
    force: bool,
    download: &Download,
) -> Result<Vec<(PathBuf, Outcome)>, String> {
    let mut out = Vec::new();
    for pin in pins {
        let path = dest.join(&pin.file);
        if !force && matches(&path, &pin.sha256) {
            out.push((path, Outcome::Present));
            continue;
        }
        let dir = path.parent().unwrap_or(dest);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tmp = dir.join(format!(
            ".{}.part",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        let result = download(&pin.url, &tmp).and_then(|()| {
            let bytes = std::fs::read(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
            let got = crate::sha256_hex(&bytes);
            if got != pin.sha256 {
                return Err(format!(
                    "{}: {} has sha256 {got}, pinned {}",
                    pin.file, pin.url, pin.sha256
                ));
            }
            std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result?;
        out.push((path, Outcome::Fetched));
    }
    Ok(out)
}

/// Check the files under `dest` against the pins without downloading anything.
pub fn verify(pins: &[Pin], dest: &Path) -> Vec<(PathBuf, Outcome)> {
    pins.iter()
        .map(|pin| {
            let path = dest.join(&pin.file);
            let outcome = match std::fs::read(&path) {
                Err(e) => Outcome::Bad(format!("missing: {e}")),
                Ok(b) if crate::sha256_hex(&b) != pin.sha256 => Outcome::Bad(format!(
                    "sha256 {} (pinned {})",
                    crate::sha256_hex(&b),
                    pin.sha256
                )),
                Ok(_) => Outcome::Present,
            };
            (path, outcome)
        })
        .collect()
}
