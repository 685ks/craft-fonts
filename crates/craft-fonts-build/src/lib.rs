//! Build-script helper that embeds the fonts of an optional craft-fonts checkout.
//!
//! It replaces the `build.rs` recipe apps used to copy (see `docs/integration.md`). Add it as a
//! **build-dependency only**, then:
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     craft_fonts_build::Builder::new().emit();
//! }
//! ```
//!
//! ```ignore
//! // in the crate that installs fonts
//! include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));
//! // CRAFT_FONTS: &[CraftFont], craft_font_chain(CraftFontRole::Ui, "Kore"), craft_font_order(..)
//! ```
//!
//! Without `CRAFT_FONTS_DIR` the generated `CRAFT_FONTS` is empty and the build carries on, so
//! craft-fonts stays an optional build input. With it, every font in the checkout's
//! `fonts/manifest.txt` is embedded with `include_bytes!` (on `wasm32`, only [`Builder::web_fonts`]),
//! together with its licence text and the manifest's fallback chains.
//!
//! The crate has no dependencies and also defines the manifest format ([`parse_manifest`]), which
//! the `craft-fonts` crate re-exports.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod parse;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub use parse::{Entry, FallbackEntry, ManifestError, Role, parse_manifest};

/// The environment variable (and build option) naming a craft-fonts checkout.
pub const DIR_ENV: &str = "CRAFT_FONTS_DIR";
/// Set (to anything) to make a bad `CRAFT_FONTS_DIR` a build error instead of a warning.
pub const REQUIRED_ENV: &str = "CRAFT_FONTS_REQUIRED";
/// The fonts embedded on `wasm32` by default: the Japanese UI font, as the old recipe did.
pub const DEFAULT_WEB_FONTS: &[(&str, &str)] = &[("BIZ UDPGothic", "Regular")];

/// `entry`'s priority in the `role` chain for `script`, if it is in that chain.
pub fn priority(entry: &Entry, role: Role, script: &str) -> Option<u16> {
    entry
        .fallback
        .iter()
        .filter(|f| f.role == role && f.script == script)
        .map(|f| f.priority)
        .min()
}

fn ranked(entries: &[Entry], role: Role, script: &str) -> Vec<usize> {
    let mut chain: Vec<(u16, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| priority(e, role, script).map(|p| (p, i)))
        .collect();
    chain.sort_by_key(|(p, _)| *p);
    chain.into_iter().map(|(_, i)| i).collect()
}

/// The indices into `entries` of the fonts to try, in order, for `role` text in `script`, ranked by
/// the manifest's fallback field (ties keep manifest order). A script with no [`Role::Mono`]
/// entry gets its [`Role::Ui`] chain for `Mono`.
pub fn fallback_chain(entries: &[Entry], role: Role, script: &str) -> Vec<usize> {
    let chain = ranked(entries, role, script);
    if chain.is_empty() && role == Role::Mono {
        return ranked(entries, Role::Ui, script);
    }
    chain
}

/// The scripts that have a `role` chain, sorted (for `Mono`, also those served by the UI chain).
pub fn fallback_scripts(entries: &[Entry], role: Role) -> Vec<String> {
    let mut scripts: Vec<String> = entries
        .iter()
        .flat_map(|e| &e.fallback)
        .filter(|f| f.role == role || (role == Role::Mono && f.role == Role::Ui))
        .map(|f| f.script.clone())
        .collect();
    scripts.sort_unstable();
    scripts.dedup();
    scripts
}

/// The indices of every font for `role` text in one fallback list: each font at its best priority
/// over [`fallback_scripts`] (ties keep manifest order). This is what a renderer with a single
/// fallback list per family (egui) wants.
pub fn fallback_order(entries: &[Entry], role: Role) -> Vec<usize> {
    let mut order: Vec<(u16, usize)> = Vec::new();
    for script in fallback_scripts(entries, role) {
        for i in fallback_chain(entries, role, &script) {
            let Some(e) = entries.get(i) else { continue };
            let p = priority(e, role, &script)
                .or_else(|| priority(e, Role::Ui, &script))
                .unwrap_or(u16::MAX);
            match order.iter_mut().find(|(_, j)| *j == i) {
                Some(slot) => slot.0 = slot.0.min(p),
                None => order.push((p, i)),
            }
        }
    }
    order.sort_unstable();
    order.into_iter().map(|(_, i)| i).collect()
}

/// A filter on manifest entries.
type Filter = Box<dyn Fn(&Entry) -> bool>;

/// Generates `$OUT_DIR/craft_fonts.rs` (by default) from the checkout named by `CRAFT_FONTS_DIR`.
pub struct Builder {
    dir: Option<Option<PathBuf>>,
    required: Option<bool>,
    out_file: String,
    web_fonts: Vec<(String, String)>,
    include: Option<Filter>,
    legacy: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    /// The defaults: read `CRAFT_FONTS_DIR` / `CRAFT_FONTS_REQUIRED`, write `craft_fonts.rs`, embed
    /// every font natively and [`DEFAULT_WEB_FONTS`] on `wasm32`.
    pub fn new() -> Self {
        Self {
            dir: None,
            required: None,
            out_file: "craft_fonts.rs".into(),
            web_fonts: DEFAULT_WEB_FONTS
                .iter()
                .map(|(f, s)| ((*f).into(), (*s).into()))
                .collect(),
            include: None,
            legacy: false,
        }
    }

    /// Use the checkout at `dir` instead of reading `CRAFT_FONTS_DIR`.
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(Some(dir.into()));
        self
    }

    /// Build without craft-fonts whatever `CRAFT_FONTS_DIR` says (e.g. for a feature that is off).
    pub fn no_checkout(mut self) -> Self {
        self.dir = Some(None);
        self
    }

    /// Whether a bad checkout fails the build, instead of reading `CRAFT_FONTS_REQUIRED`.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// The file name written in `OUT_DIR` (default `craft_fonts.rs`).
    pub fn out_file(mut self, name: impl Into<String>) -> Self {
        self.out_file = name.into();
        self
    }

    /// The `(family, style)` pairs embedded on `wasm32` (web builds have per-file size limits, e.g.
    /// Cloudflare's 25 MiB). Pass an empty slice to embed nothing there.
    pub fn web_fonts(mut self, fonts: &[(&str, &str)]) -> Self {
        self.web_fonts = fonts
            .iter()
            .map(|(f, s)| ((*f).into(), (*s).into()))
            .collect();
        self
    }

    /// Embed only the entries `keep` accepts (on every target, after [`Self::web_fonts`]), e.g.
    /// `|e| e.scripts.iter().any(|s| s == "Jpan")`.
    pub fn include(mut self, keep: impl Fn(&Entry) -> bool + 'static) -> Self {
        self.include = Some(Box::new(keep));
        self
    }

    /// Emit only `CraftFont { family, style, scripts, bytes }` initialisers for an app that
    /// defines `CraftFont` itself, exactly as the old copied recipe did (no chains, no types).
    pub fn legacy(mut self, legacy: bool) -> Self {
        self.legacy = legacy;
        self
    }

    /// Run from `build.rs`: print the `cargo::` directives and write the file. Never panics; a bad
    /// checkout is a `cargo::warning` (and an empty font list), or a `cargo::error` when required.
    pub fn emit(self) {
        let dir = match &self.dir {
            Some(d) => d.clone(),
            None => {
                println!("cargo::rerun-if-env-changed={DIR_ENV}");
                std::env::var_os(DIR_ENV).map(PathBuf::from)
            }
        };
        let required = self.required.unwrap_or_else(|| {
            println!("cargo::rerun-if-env-changed={REQUIRED_ENV}");
            std::env::var_os(REQUIRED_ENV).is_some()
        });
        let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
        let mut rerun = Vec::new();
        let src = match self.generate(dir.as_deref(), wasm, &mut rerun) {
            Ok(src) => src,
            Err(e) => {
                let shown = dir
                    .as_deref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                if required {
                    println!("cargo::error={DIR_ENV}={shown}: {e}");
                } else {
                    println!("cargo::warning=building without craft-fonts: {DIR_ENV}={shown}: {e}");
                }
                self.render(&[], &[])
            }
        };
        for path in rerun {
            println!("cargo::rerun-if-changed={}", path.display());
        }
        let Some(out_dir) = std::env::var_os("OUT_DIR") else {
            println!("cargo::error=craft-fonts-build: OUT_DIR is not set (call it from build.rs)");
            return;
        };
        let out = PathBuf::from(out_dir).join(&self.out_file);
        if let Err(e) = std::fs::write(&out, src) {
            println!("cargo::error=writing {}: {e}", out.display());
        }
    }

    /// The generated source for the checkout at `dir` (`None`: none), for a `wasm32` target or
    /// not. Files the output depends on are pushed to `rerun`.
    pub fn generate(
        &self,
        dir: Option<&Path>,
        wasm: bool,
        rerun: &mut Vec<PathBuf>,
    ) -> Result<String, String> {
        let Some(dir) = dir else {
            return Ok(self.render(&[], &[]));
        };
        let manifest = dir.join("fonts/manifest.txt");
        rerun.push(manifest.clone());
        let text = std::fs::read_to_string(&manifest)
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        let entries: Vec<Entry> = parse_manifest(&text)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|e| {
                !wasm
                    || self
                        .web_fonts
                        .iter()
                        .any(|(f, s)| *f == e.family && *s == e.style)
            })
            .filter(|e| self.include.as_ref().is_none_or(|keep| keep(e)))
            .collect();
        let mut files = Vec::new();
        for e in &entries {
            let canon = |rel: &str| {
                dir.join(rel)
                    .canonicalize()
                    .map_err(|err| format!("{rel}: {err}"))
            };
            let font = canon(&e.file)?;
            let licence = canon(&e.licence_file)?;
            rerun.push(font.clone());
            rerun.push(licence.clone());
            files.push((font, licence));
        }
        Ok(self.render(&entries, &files))
    }

    fn render(&self, entries: &[Entry], files: &[(PathBuf, PathBuf)]) -> String {
        let mut src = String::from(
            "// @generated by craft-fonts-build from an optional craft-fonts checkout; empty without one.\n",
        );
        if !self.legacy {
            src.push_str(TYPES);
        }
        src.push_str("#[allow(dead_code)]\npub static CRAFT_FONTS: &[CraftFont] = &[\n");
        for (e, (font, licence)) in entries.iter().zip(files) {
            let scripts: Vec<String> = e.scripts.iter().map(|s| format!("{s:?}")).collect();
            let font = font.display().to_string();
            if self.legacy {
                let _ = writeln!(
                    src,
                    "    CraftFont {{ family: {:?}, style: {:?}, scripts: &[{}], bytes: include_bytes!({font:?}) }},",
                    e.family,
                    e.style,
                    scripts.join(", "),
                );
                continue;
            }
            let fallback: Vec<String> = e
                .fallback
                .iter()
                .map(|f| {
                    format!(
                        "CraftFallback {{ role: CraftFontRole::{:?}, script: {:?}, priority: {} }}",
                        f.role, f.script, f.priority
                    )
                })
                .collect();
            let _ = writeln!(
                src,
                "    CraftFont {{ family: {:?}, style: {:?}, scripts: &[{}], fallback: &[{}], licence: {:?}, licence_text: include_str!({:?}), bytes: include_bytes!({font:?}) }},",
                e.family,
                e.style,
                scripts.join(", "),
                fallback.join(", "),
                e.licence,
                licence.display().to_string(),
            );
        }
        src.push_str("];\n");
        if self.legacy {
            return src;
        }
        src.push_str("#[allow(dead_code)]\nstatic CRAFT_FONT_CHAINS: &[(CraftFontRole, &str, &[usize])] = &[\n");
        for role in Role::ALL {
            for script in fallback_scripts(entries, role) {
                let chain = fallback_chain(entries, role, &script);
                let _ = writeln!(
                    src,
                    "    (CraftFontRole::{role:?}, {script:?}, &{chain:?}),"
                );
            }
        }
        src.push_str("];\n#[allow(dead_code)]\nstatic CRAFT_FONT_ORDERS: &[(CraftFontRole, &[usize])] = &[\n");
        for role in Role::ALL {
            let order = fallback_order(entries, role);
            let _ = writeln!(src, "    (CraftFontRole::{role:?}, &{order:?}),");
        }
        src.push_str("];\n");
        src.push_str(FUNCTIONS);
        src
    }
}

const TYPES: &str = r#"/// What kind of text a fallback chain is for.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CraftFontRole {
    /// Interface text.
    Ui,
    /// Document text.
    Document,
    /// Monospace text (scripts without a monospace font use their UI chain).
    Mono,
}
/// One place of a font in a fallback chain (lower `priority` first, global across scripts).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CraftFallback {
    pub role: CraftFontRole,
    pub script: &'static str,
    pub priority: u16,
}
/// A font from the optional craft-fonts build input.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct CraftFont {
    pub family: &'static str,
    pub style: &'static str,
    /// ISO 15924 scripts the font is for, e.g. `"Jpan"`.
    pub scripts: &'static [&'static str],
    pub fallback: &'static [CraftFallback],
    /// SPDX licence identifier; `licence_text` must ship with the app.
    pub licence: &'static str,
    pub licence_text: &'static str,
    pub bytes: &'static [u8],
}
"#;

const FUNCTIONS: &str = r#"/// The embedded fonts to try, in order, for `role` text in `script` (e.g. `"Kore"`).
#[allow(dead_code)]
pub fn craft_font_chain(role: CraftFontRole, script: &str) -> impl Iterator<Item = &'static CraftFont> + '_ {
    CRAFT_FONT_CHAINS
        .iter()
        .filter(move |(r, s, _)| *r == role && *s == script)
        .flat_map(|(_, _, chain)| chain.iter())
        .filter_map(|i| CRAFT_FONTS.get(*i))
}
/// Every embedded font for `role` text, in one fallback order (e.g. for an egui font family).
#[allow(dead_code)]
pub fn craft_font_order(role: CraftFontRole) -> impl Iterator<Item = &'static CraftFont> {
    CRAFT_FONT_ORDERS
        .iter()
        .filter(move |(r, _)| *r == role)
        .flat_map(|(_, order)| order.iter())
        .filter_map(|i| CRAFT_FONTS.get(*i))
}
/// The scripts with a `role` chain among the embedded fonts, sorted.
#[allow(dead_code)]
pub fn craft_font_scripts(role: CraftFontRole) -> impl Iterator<Item = &'static str> {
    CRAFT_FONT_CHAINS
        .iter()
        .filter(move |(r, _, _)| *r == role)
        .map(|(_, s, _)| *s)
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(name: &str, manifest: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "craft-fonts-build-test-{}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("fonts/a")).unwrap();
        std::fs::write(dir.join("fonts/manifest.txt"), manifest).unwrap();
        std::fs::write(dir.join("fonts/a/A.ttf"), b"font").unwrap();
        std::fs::write(dir.join("fonts/a/OFL.txt"), b"licence").unwrap();
        dir
    }

    fn line(family: &str, file: &str, fallback: &str) -> String {
        format!(
            "{family} | Regular | {file} | Jpan,Latn | OFL-1.1 | fonts/a/OFL.txt | {} | src | {fallback}\n",
            "0".repeat(64)
        )
    }

    #[test]
    fn without_a_checkout_the_list_is_empty_but_typed() {
        let src = Builder::new()
            .generate(None, false, &mut Vec::new())
            .unwrap();
        assert!(src.contains("pub struct CraftFont"));
        assert!(src.contains("pub static CRAFT_FONTS: &[CraftFont] = &[\n];"));
        assert!(src.contains("fn craft_font_chain"));
    }

    #[test]
    fn a_checkout_is_embedded_with_its_chains() {
        let manifest = line("A", "fonts/a/A.ttf", "ui/Jpan=20,doc/Latn=5")
            + &line("B", "fonts/a/A.ttf", "ui/Jpan=10");
        let dir = checkout("chains", &manifest);
        let mut rerun = Vec::new();
        let src = Builder::new()
            .generate(Some(&dir), false, &mut rerun)
            .unwrap();
        assert!(src.contains("family: \"A\""), "{src}");
        assert!(src.contains("include_bytes!("));
        assert!(src.contains("licence_text: include_str!("));
        assert!(
            src.contains("(CraftFontRole::Ui, \"Jpan\", &[1, 0]),"),
            "{src}"
        );
        assert!(
            src.contains("(CraftFontRole::Mono, \"Jpan\", &[1, 0]),"),
            "{src}"
        );
        assert!(src.contains("(CraftFontRole::Document, &[0]),"), "{src}");
        assert!(rerun.iter().any(|p| p.ends_with("manifest.txt")));
        // On the web only the listed fonts are embedded, and chains index the filtered list.
        let web = Builder::new()
            .web_fonts(&[("A", "Regular")])
            .generate(Some(&dir), true, &mut Vec::new())
            .unwrap();
        assert!(!web.contains("family: \"B\""));
        assert!(
            web.contains("(CraftFontRole::Ui, \"Jpan\", &[0]),"),
            "{web}"
        );
        let filtered = Builder::new()
            .include(|e| e.family == "B")
            .generate(Some(&dir), false, &mut Vec::new())
            .unwrap();
        assert!(!filtered.contains("family: \"A\""));
        let legacy = Builder::new()
            .legacy(true)
            .generate(Some(&dir), false, &mut Vec::new())
            .unwrap();
        assert!(!legacy.contains("struct CraftFont"));
        assert!(!legacy.contains("fallback"));
        assert!(legacy.contains("scripts: &[\"Jpan\", \"Latn\"], bytes: include_bytes!("));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_bad_checkout_is_an_error_not_a_panic() {
        let b = Builder::new();
        assert!(
            b.generate(Some(Path::new("/nonexistent")), false, &mut Vec::new())
                .is_err()
        );
        let dir = checkout("bad", &line("A", "fonts/a/Missing.ttf", "-"));
        let err = b.generate(Some(&dir), false, &mut Vec::new()).unwrap_err();
        assert!(err.contains("Missing.ttf"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
