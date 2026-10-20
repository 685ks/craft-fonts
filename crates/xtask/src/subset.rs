//! `cargo xtask subset`: cut fonts down to the characters a web/WASM build needs.
//!
//! Subsetting is done by [`skera`], the fontations port of HarfBuzz's `hb-subset`: layout
//! (GSUB/GPOS) closure, variable-font tables and COLRv1 are kept, so shaping (Arabic, Devanagari)
//! and variations still work on the result.

use std::path::{Path, PathBuf};

use craft_fonts_build::{Entry, parse_manifest};
use skera::{DEFAULT_LAYOUT_FEATURES, Plan, SubsetFlags, subset_font};
use write_fonts::read::FontRef;
use write_fonts::read::collections::IntSet;
use write_fonts::read::types::{GlyphId, NameId, Tag};

/// Parse Unicode ranges: comma- or whitespace-separated `U+0041`, `41`, `U+0020-U+007E` or
/// `0020-007E` (`..` also works as the range separator).
pub fn parse_ranges(spec: &str) -> Result<Vec<(u32, u32)>, String> {
    let hex = |s: &str| {
        let t = s.trim();
        let t = t
            .strip_prefix("U+")
            .or_else(|| t.strip_prefix("u+"))
            .or_else(|| t.strip_prefix("0x"))
            .unwrap_or(t);
        u32::from_str_radix(t, 16)
            .ok()
            .filter(|c| *c <= 0x10_FFFF)
            .ok_or_else(|| format!("{s:?} is not a code point (want e.g. U+0041)"))
    };
    spec.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|token| {
            let (a, b) = token
                .split_once("..")
                .or_else(|| token.split_once('-'))
                .unwrap_or((token, token));
            let (a, b) = (hex(a)?, hex(b)?);
            if a > b {
                return Err(format!("{token}: the range is backwards"));
            }
            Ok((a, b))
        })
        .collect()
}

/// The code points to keep: every character of `texts` plus every range in `ranges`.
pub fn codepoints(texts: &[String], ranges: &[String]) -> Result<IntSet<u32>, String> {
    let mut set = IntSet::empty();
    for text in texts {
        set.extend(text.chars().map(u32::from));
    }
    for spec in ranges {
        for (a, b) in parse_ranges(spec)? {
            set.insert_range(a..=b);
        }
    }
    Ok(set)
}

/// Subset one font to `codepoints` (plus whatever its layout rules reach from them).
pub fn subset(font_bytes: &[u8], codepoints: &IntSet<u32>) -> Result<Vec<u8>, String> {
    let font = FontRef::new(font_bytes).map_err(|e| format!("not a font: {e}"))?;
    // HarfBuzz's defaults (see skera's CLI), except that the typographic family names (16, 17)
    // are kept: apps look fonts up by them.
    let drop_tables: IntSet<Tag> = [
        skera::MORX,
        skera::MORT,
        skera::KERX,
        skera::KERN,
        skera::JSTF,
        skera::DSIG,
        Tag::new(b"EBDT"),
        Tag::new(b"EBLC"),
        skera::EBSC,
        Tag::new(b"SVG "),
        skera::PCLT,
        skera::LTSH,
        Tag::new(b"feat"),
        skera::GLAT,
        skera::GLOC,
        skera::SILF,
        skera::SILL,
    ]
    .into_iter()
    .collect();
    let mut scripts = IntSet::<Tag>::empty();
    scripts.invert();
    let features: IntSet<Tag> = DEFAULT_LAYOUT_FEATURES.iter().copied().collect();
    let mut name_ids = IntSet::<NameId>::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));
    name_ids.insert(NameId::from(16));
    name_ids.insert(NameId::from(17));
    let mut languages = IntSet::<u16>::empty();
    languages.insert(0x0409);
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        codepoints,
        &font,
        SubsetFlags::default(),
        &drop_tables,
        &scripts,
        &features,
        &name_ids,
        &languages,
    );
    subset_font(&font, &plan).map_err(|e| format!("subsetting failed: {e}"))
}

/// One subset font written by [`subset_fonts`].
#[derive(Debug)]
pub struct Written {
    pub path: PathBuf,
    pub before: usize,
    pub after: usize,
}

/// The manifest entries `selectors` name: a family name (every style), a manifest file path, or
/// `all`.
pub fn select<'a>(entries: &'a [Entry], selectors: &[String]) -> Result<Vec<&'a Entry>, String> {
    let mut out: Vec<&Entry> = Vec::new();
    for s in selectors {
        let hits: Vec<&Entry> = entries
            .iter()
            .filter(|e| s == "all" || e.family == *s || e.file == *s)
            .collect();
        if hits.is_empty() {
            return Err(format!(
                "{s:?} is not a family or file in fonts/manifest.txt (or `all`)"
            ));
        }
        for e in hits {
            if !out.contains(&e) {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// Subset the manifest fonts `selectors` names from the checkout at `root` into `out`.
///
/// With `checkout`, `out` becomes a craft-fonts checkout of its own (same paths, the licences, and
/// a `fonts/manifest.txt` with the subset files' checksums), so a web build can point
/// `CRAFT_FONTS_DIR` at it. Otherwise each subset font is written as `out/<file name>`.
pub fn subset_fonts(
    root: &Path,
    selectors: &[String],
    codepoints: &IntSet<u32>,
    out: &Path,
    checkout: bool,
) -> Result<Vec<Written>, String> {
    let manifest_path = root.join("fonts/manifest.txt");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let entries = parse_manifest(&text).map_err(|e| e.to_string())?;
    let chosen = select(&entries, selectors)?;
    let mut written = Vec::new();
    let mut manifest = String::from(
        "# Subset by `cargo xtask subset` from a craft-fonts checkout; same format as the original.\n",
    );
    for e in chosen {
        let bytes =
            std::fs::read(root.join(&e.file)).map_err(|err| format!("{}: {err}", e.file))?;
        let small = subset(&bytes, codepoints).map_err(|err| format!("{}: {err}", e.file))?;
        let path = if checkout {
            out.join(&e.file)
        } else {
            out.join(Path::new(&e.file).file_name().unwrap_or_default())
        };
        write(&path, &small)?;
        if checkout {
            let licence = std::fs::read(root.join(&e.licence_file))
                .map_err(|err| format!("{}: {err}", e.licence_file))?;
            write(&out.join(&e.licence_file), &licence)?;
            manifest.push_str(&manifest_line(e, &crate::sha256_hex(&small)));
        }
        written.push(Written {
            path,
            before: bytes.len(),
            after: small.len(),
        });
    }
    if checkout {
        write(&out.join("fonts/manifest.txt"), manifest.as_bytes())?;
    }
    Ok(written)
}

fn manifest_line(e: &Entry, sha256: &str) -> String {
    let fallback: Vec<String> = e
        .fallback
        .iter()
        .map(|f| format!("{}/{}={}", f.role.as_str(), f.script, f.priority))
        .collect();
    let fallback = if fallback.is_empty() {
        "-".to_string()
    } else {
        fallback.join(",")
    };
    format!(
        "{} | {} | {} | {} | {} | {} | {sha256} | {} (subset) | {fallback}\n",
        e.family,
        e.style,
        e.file,
        e.scripts.join(","),
        e.licence,
        e.licence_file,
        e.source
    )
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
