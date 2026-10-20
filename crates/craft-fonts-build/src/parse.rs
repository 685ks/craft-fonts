//! The `fonts/manifest.txt` format. It has no dependencies, so the `craft-fonts-build` helper and
//! the `craft-fonts` crate (which re-exports it) share one definition.

/// One line of the manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Family name, as the font's own `name` table spells it (e.g. `Shippori Mincho`).
    pub family: String,
    /// Style within the family (e.g. `Regular`, `Bold`).
    pub style: String,
    /// Path of the font file, relative to the repository root.
    pub file: String,
    /// ISO 15924 script codes the font is meant for (e.g. `Jpan`, `Latn`).
    pub scripts: Vec<String>,
    /// SPDX licence identifier.
    pub licence: String,
    /// Path of the licence text, relative to the repository root.
    pub licence_file: String,
    /// Lower-case hex SHA-256 of the font file.
    pub sha256: String,
    /// Where the file came from (a pinned upstream URL).
    pub source: String,
    /// The font's places in the per-script fallback chains (the optional 9th field).
    pub fallback: Vec<FallbackEntry>,
}

/// What kind of text a fallback chain is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// Interface text: menus, labels, buttons (`ui` in the manifest).
    Ui,
    /// Document text: what the user writes and prints (`doc`).
    Document,
    /// Monospace text: code, terminals, numeric columns (`mono`). A script with no `mono` entry
    /// falls back to its `ui` chain.
    Mono,
}

impl Role {
    /// Every role, in manifest-documentation order.
    pub const ALL: [Role; 3] = [Role::Ui, Role::Document, Role::Mono];

    /// The manifest spelling: `ui`, `doc` or `mono`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Ui => "ui",
            Role::Document => "doc",
            Role::Mono => "mono",
        }
    }

    /// The role spelled `s` in the manifest, if any.
    pub fn from_manifest(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

/// One `role/Script=priority` entry of the fallback field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FallbackEntry {
    pub role: Role,
    /// ISO 15924 script code; always one of the font's `scripts`.
    pub script: String,
    /// Lower comes first. Global: it orders fonts across scripts as well as within one.
    pub priority: u16,
}

/// Why a manifest line could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// The line does not have 8 or 9 `" | "`-separated fields.
    FieldCount { line: usize, found: usize },
    /// A field is empty.
    EmptyField { line: usize, field: &'static str },
    /// The SHA-256 is not 64 lower-case hex digits.
    BadSha256 { line: usize },
    /// A fallback entry is not `role/Script=priority` with a known role, one of the font's
    /// scripts and a number.
    BadFallback { line: usize, entry: String },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FieldCount { line, found } => write!(
                f,
                "manifest line {line}: expected 8 or 9 fields separated by \" | \", found {found}"
            ),
            Self::EmptyField { line, field } => {
                write!(f, "manifest line {line}: the {field} field is empty")
            }
            Self::BadSha256 { line } => write!(
                f,
                "manifest line {line}: sha256 must be 64 lower-case hex digits"
            ),
            Self::BadFallback { line, entry } => write!(
                f,
                "manifest line {line}: bad fallback entry {entry:?} (want role/Script=priority, \
                 role ui, doc or mono, Script one of the font's scripts)"
            ),
        }
    }
}

impl std::error::Error for ManifestError {}

const FIELDS: [&str; 8] = [
    "family",
    "style",
    "file",
    "scripts",
    "licence",
    "licence file",
    "sha256",
    "source",
];

/// Parse the whole manifest. Blank lines and `#` comments are skipped; line numbers are 1-based.
pub fn parse_manifest(text: &str) -> Result<Vec<Entry>, ManifestError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut fields: Vec<&str> = trimmed.split(" | ").map(str::trim).collect();
        let fallback_field = if fields.len() == 9 {
            fields.pop()
        } else {
            None
        };
        let [
            family,
            style,
            file,
            scripts,
            licence,
            licence_file,
            sha256,
            source,
        ] = fields.as_slice()
        else {
            return Err(ManifestError::FieldCount {
                line,
                found: fields.len(),
            });
        };
        for (name, value) in FIELDS.iter().zip([
            family,
            style,
            file,
            scripts,
            licence,
            licence_file,
            sha256,
            source,
        ]) {
            if value.is_empty() {
                return Err(ManifestError::EmptyField { line, field: name });
            }
        }
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ManifestError::BadSha256 { line });
        }
        let scripts: Vec<String> = scripts
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let fallback = parse_fallback(line, fallback_field.unwrap_or("-"), &scripts)?;
        out.push(Entry {
            family: (*family).to_string(),
            style: (*style).to_string(),
            file: (*file).to_string(),
            scripts,
            licence: (*licence).to_string(),
            licence_file: (*licence_file).to_string(),
            sha256: (*sha256).to_string(),
            source: (*source).to_string(),
            fallback,
        });
    }
    Ok(out)
}

fn parse_fallback(
    line: usize,
    field: &str,
    scripts: &[String],
) -> Result<Vec<FallbackEntry>, ManifestError> {
    if field.is_empty() || field == "-" {
        return Ok(Vec::new());
    }
    field
        .split(',')
        .map(str::trim)
        .map(|entry| {
            let bad = || ManifestError::BadFallback {
                line,
                entry: entry.to_string(),
            };
            let (role, rest) = entry.split_once('/').ok_or_else(bad)?;
            let (script, priority) = rest.split_once('=').ok_or_else(bad)?;
            let role = Role::from_manifest(role.trim()).ok_or_else(bad)?;
            let script = script.trim();
            if !scripts.iter().any(|s| s == script) {
                return Err(bad());
            }
            let priority = priority.trim().parse().map_err(|_| bad())?;
            Ok(FallbackEntry {
                role,
                script: script.to_string(),
                priority,
            })
        })
        .collect()
}
