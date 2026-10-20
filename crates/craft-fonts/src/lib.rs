//! The Crafting Apps' shared font assets.
//!
//! The fonts live in this repository's `fonts/` directory and are listed in `fonts/manifest.txt`.
//! This crate exposes that manifest ([`FONTS`]), finds a craft-fonts checkout at run time
//! ([`dir_from_env`]), loads fonts from one ([`load`]), and with the `embed` feature compiles them
//! into the binary ([`embedded`]).
//!
//! The apps do **not** depend on this crate: Cargo resolves even optional git dependencies, so a
//! dependency would make craft-fonts required for every build. Apps read `fonts/manifest.txt`
//! from their own `build.rs` when the `CRAFT_FONTS_DIR` build option is set; see
//! `docs/integration.md`; the `craft-fonts-build` helper does that for them. This crate is for
//! tools and tests that can afford the dependency. The manifest format ([`parse_manifest`]) and
//! the chain ranking live in `craft-fonts-build` and are re-exported here.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::{Path, PathBuf};

pub use craft_fonts_build::{Entry, FallbackEntry, ManifestError, Role, parse_manifest};

/// One font file from `fonts/manifest.txt`, known at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Font {
    /// Family name, as the font's own `name` table spells it.
    pub family: &'static str,
    /// Style within the family (`Regular`, `Bold`, …).
    pub style: &'static str,
    /// Path of the font file, relative to the repository root.
    pub file: &'static str,
    /// ISO 15924 script codes the font is meant for.
    pub scripts: &'static [&'static str],
    /// SPDX licence identifier.
    pub licence: &'static str,
    /// Path of the licence text, relative to the repository root.
    pub licence_file: &'static str,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: &'static str,
    /// Pinned upstream source.
    pub source: &'static str,
    /// The font's places in the per-script fallback chains.
    pub fallback: &'static [Fallback],
}

/// One place of a font in a fallback chain: for `role` text in `script`, at `priority`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fallback {
    pub role: Role,
    /// ISO 15924 script code.
    pub script: &'static str,
    /// Lower comes first; global across scripts.
    pub priority: u16,
}

impl Font {
    /// This font's priority in the `role` chain for `script`, if it is in that chain.
    pub fn priority(&self, role: Role, script: &str) -> Option<u16> {
        self.fallback
            .iter()
            .filter(|f| f.role == role && f.script == script)
            .map(|f| f.priority)
            .min()
    }
}

include!(concat!(env!("OUT_DIR"), "/manifest.rs"));

/// The fonts meant for an ISO 15924 script (e.g. `"Jpan"`), in manifest order.
pub fn for_script(script: &str) -> impl Iterator<Item = &'static Font> + '_ {
    FONTS.iter().filter(move |f| f.scripts.contains(&script))
}

/// The fonts to try, in order, for `role` text in `script` (e.g. `Role::Ui`, `"Kore"`), as the
/// manifest's fallback field ranks them (ties keep manifest order). Every style of a family is
/// listed; pick the one you need by [`Font::style`]. A script with no [`Role::Mono`] entry gets its
/// [`Role::Ui`] chain for `Mono`. Empty for a script no font is ranked for.
pub fn fallback_chain(role: Role, script: &str) -> Vec<&'static Font> {
    CHAINS
        .iter()
        .filter(|(r, s, _)| *r == role && *s == script)
        .flat_map(|(_, _, chain)| chain.iter())
        .filter_map(|i| FONTS.get(*i))
        .collect()
}

/// The scripts that have a `role` chain, sorted (for `Mono`, also those served by the UI chain).
pub fn fallback_scripts(role: Role) -> Vec<&'static str> {
    CHAINS
        .iter()
        .filter(|(r, _, _)| *r == role)
        .map(|(_, s, _)| *s)
        .collect()
}

/// One list of every font for `role` text, in fallback order: each font at its best priority over
/// [`fallback_scripts`]. This is what a renderer with a single fallback list per family (egui)
/// wants: e.g. Inter, then the Japanese, Chinese, Korean and other-script fonts, then emoji.
pub fn fallback_order(role: Role) -> Vec<&'static Font> {
    ORDERS
        .iter()
        .filter(|(r, _)| *r == role)
        .flat_map(|(_, order)| order.iter())
        .filter_map(|i| FONTS.get(*i))
        .collect()
}

/// The compiled-in bytes of a font from [`FONTS`] (`embed` feature). `None` for any other value.
#[cfg(feature = "embed")]
pub fn embedded(font: &Font) -> Option<&'static [u8]> {
    FONTS
        .iter()
        .position(|f| f == font)
        .and_then(|i| EMBEDDED.get(i).copied())
}

/// The environment variable (and build option) naming a craft-fonts checkout.
pub const DIR_ENV: &str = "CRAFT_FONTS_DIR";

/// The craft-fonts checkout named by `CRAFT_FONTS_DIR`, if it is set and holds a manifest.
pub fn dir_from_env() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os(DIR_ENV)?);
    dir.join("fonts/manifest.txt").is_file().then_some(dir)
}

/// Why fonts could not be loaded from a checkout.
#[derive(Debug)]
pub enum LoadError {
    /// A file could not be read.
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    /// The manifest is malformed.
    Manifest(ManifestError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "reading {}: {error}", path.display()),
            Self::Manifest(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for LoadError {}

/// Read the manifest of the checkout at `dir` and every font it lists, in manifest order.
pub fn load(dir: &Path) -> Result<Vec<(Entry, Vec<u8>)>, LoadError> {
    let manifest = dir.join("fonts/manifest.txt");
    let text = std::fs::read_to_string(&manifest).map_err(|error| LoadError::Io {
        path: manifest,
        error,
    })?;
    let entries = parse_manifest(&text).map_err(LoadError::Manifest)?;
    entries
        .into_iter()
        .map(|e| {
            let path = dir.join(&e.file);
            std::fs::read(&path)
                .map(|bytes| (e, bytes))
                .map_err(|error| LoadError::Io { path, error })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_parses_into_the_static_table() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text = std::fs::read_to_string(root.join("fonts/manifest.txt")).unwrap();
        let entries = parse_manifest(&text).unwrap();
        assert_eq!(entries.len(), FONTS.len());
        for (e, f) in entries.iter().zip(FONTS) {
            assert_eq!(
                (e.family.as_str(), e.style.as_str(), e.file.as_str()),
                (f.family, f.style, f.file)
            );
        }
        assert!(for_script("Jpan").count() >= 1);
    }

    #[test]
    fn malformed_lines_are_errors_not_panics() {
        assert_eq!(
            parse_manifest("a | b"),
            Err(ManifestError::FieldCount { line: 1, found: 2 })
        );
        let blank_style = format!("A |  | f | Jpan | OFL-1.1 | l | {} | s", "0".repeat(64));
        assert_eq!(
            parse_manifest(&blank_style),
            Err(ManifestError::EmptyField {
                line: 1,
                field: "style"
            })
        );
        assert_eq!(
            parse_manifest("A | B | f | Jpan | OFL-1.1 | l | XYZ | s"),
            Err(ManifestError::BadSha256 { line: 1 })
        );
        assert_eq!(parse_manifest("# comment\n\n"), Ok(Vec::new()));
    }

    #[test]
    fn fallback_entries_parse_and_are_checked() {
        let line = |fb: &str| {
            format!(
                "A | Regular | f | Jpan,Latn | OFL-1.1 | l | {} | s | {fb}",
                "0".repeat(64)
            )
        };
        let e = parse_manifest(&line("ui/Jpan=20, doc/Latn=5")).unwrap();
        assert_eq!(
            e[0].fallback,
            vec![
                FallbackEntry {
                    role: Role::Ui,
                    script: "Jpan".into(),
                    priority: 20
                },
                FallbackEntry {
                    role: Role::Document,
                    script: "Latn".into(),
                    priority: 5
                },
            ]
        );
        assert_eq!(parse_manifest(&line("-")).unwrap()[0].fallback, vec![]);
        for bad in ["ui/Hans=1", "web/Jpan=1", "ui/Jpan", "ui/Jpan=x", "Jpan=1"] {
            assert!(
                matches!(
                    parse_manifest(&line(bad)),
                    Err(ManifestError::BadFallback { line: 1, .. })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn fallback_chains_follow_the_priorities() {
        for role in Role::ALL {
            for script in fallback_scripts(role) {
                let chain = fallback_chain(role, script);
                assert!(!chain.is_empty(), "{role:?} {script}");
                let own = chain.iter().any(|f| f.priority(role, script).is_some());
                let ranks: Vec<u16> = chain
                    .iter()
                    .map(|f| {
                        f.priority(if own { role } else { Role::Ui }, script)
                            .unwrap()
                    })
                    .collect();
                assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{role:?} {script}");
            }
        }
        // Each font appears once in an order, and every chained font is in it.
        for role in Role::ALL {
            let order = fallback_order(role);
            for (i, f) in order.iter().enumerate() {
                assert!(
                    !order[i + 1..].iter().any(|g| g == f),
                    "{role:?}: {f:?} twice"
                );
            }
            for script in fallback_scripts(role) {
                for f in fallback_chain(role, script) {
                    assert!(order.contains(&f), "{role:?} {script}: {}", f.family);
                }
            }
        }
        assert!(fallback_chain(Role::Ui, "Xxxx").is_empty());
    }

    #[test]
    fn a_missing_checkout_is_an_error() {
        assert!(matches!(
            load(Path::new("/nonexistent/craft-fonts")),
            Err(LoadError::Io { .. })
        ));
    }

    #[cfg(feature = "embed")]
    #[test]
    fn embedded_bytes_match_the_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for f in FONTS {
            assert_eq!(
                embedded(f).unwrap(),
                std::fs::read(root.join(f.file)).unwrap().as_slice(),
                "{}",
                f.file
            );
        }
    }
}
