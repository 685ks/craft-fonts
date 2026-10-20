//! `cargo xtask subset` on a small font (Noto Sans Hebrew, ~110 KB).

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider, string::StringId};
use xtask::subset::{codepoints, parse_ranges, subset, subset_fonts};

const HEBREW: &str = "fonts/noto-sans-hebrew/NotoSansHebrew.ttf";

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-subset-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

struct Count(usize);
impl OutlinePen for Count {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn close(&mut self) {}
}

#[test]
fn ranges_parse_in_the_usual_spellings() {
    assert_eq!(
        parse_ranges("U+0041, 61-7a U+0020..U+0021 0x30").unwrap(),
        vec![(0x41, 0x41), (0x61, 0x7a), (0x20, 0x21), (0x30, 0x30)]
    );
    assert!(parse_ranges("U+0042-U+0041").is_err());
    assert!(parse_ranges("hello").is_err());
    assert!(parse_ranges("U+110000").is_err());
    let set = codepoints(&["שלום".into()], &["U+0030-U+0039".into()]).unwrap();
    assert_eq!(set.len(), 4 + 10);
}

#[test]
fn a_subset_keeps_exactly_the_requested_characters_and_still_draws() {
    let original = std::fs::read(xtask::repo_root().join(HEBREW)).unwrap();
    let keep = codepoints(&["שלום עולם".into()], &["U+0030-U+0039".into()]).unwrap();
    let small = subset(&original, &keep).unwrap();
    assert!(
        small.len() * 2 < original.len(),
        "{} -> {}",
        original.len(),
        small.len()
    );

    let font = FontRef::new(&small).unwrap();
    let charmap = font.charmap();
    for c in "שלוםע 0123456789".chars() {
        assert!(charmap.map(c).is_some(), "lost {c:?}");
    }
    for c in "אבגדת".chars() {
        assert!(
            charmap.map(c).is_none(),
            "kept {c:?}, which was not asked for"
        );
    }
    // Outlines survive, at the default and at a non-default variation location.
    assert!(font.fvar().is_ok(), "the variation axes were dropped");
    let outlines = font.outline_glyphs();
    let bold = font.axes().location([("wght", 800.0)]);
    for location in [LocationRef::default(), (&bold).into()] {
        let gid = charmap.map('ש').unwrap();
        let mut pen = Count(0);
        outlines
            .get(gid)
            .unwrap()
            .draw(DrawSettings::unhinted(Size::new(32.0), location), &mut pen)
            .unwrap();
        assert!(pen.0 > 0, "ש draws nothing");
    }
    // The family name apps look it up by is kept.
    let names: Vec<String> = [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]
        .into_iter()
        .flat_map(|id| {
            font.localized_strings(id)
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(names.iter().any(|n| n == "Noto Sans Hebrew"), "{names:?}");
}

#[test]
fn a_subset_checkout_works_as_a_craft_fonts_dir() {
    let out = scratch("checkout");
    let keep = codepoints(&["שלום".into()], &[]).unwrap();
    let written = subset_fonts(
        &xtask::repo_root(),
        &["Noto Sans Hebrew".into()],
        &keep,
        &out,
        true,
    )
    .unwrap();
    assert_eq!(written.len(), 1);
    assert!(written[0].after < written[0].before);

    let text = std::fs::read_to_string(out.join("fonts/manifest.txt")).unwrap();
    let entries = craft_fonts_build::parse_manifest(&text).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.family, "Noto Sans Hebrew");
    assert_eq!(
        e.sha256,
        xtask::sha256_hex(&std::fs::read(out.join(&e.file)).unwrap())
    );
    assert!(!e.fallback.is_empty(), "the fallback ranks were dropped");
    assert!(
        std::fs::read_to_string(out.join(&e.licence_file))
            .unwrap()
            .contains("SIL OPEN FONT LICENSE Version 1.1")
    );
    // The build helper accepts it like a full checkout.
    let src = craft_fonts_build::Builder::new()
        .generate(Some(&out), true, &mut Vec::new())
        .unwrap();
    assert!(
        !src.contains("Noto Sans Hebrew"),
        "not a web font by default"
    );
    let src = craft_fonts_build::Builder::new()
        .web_fonts(&[("Noto Sans Hebrew", "Regular")])
        .generate(Some(&out), true, &mut Vec::new())
        .unwrap();
    assert!(src.contains("family: \"Noto Sans Hebrew\""));
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn unknown_fonts_are_errors() {
    let keep = codepoints(&["a".into()], &[]).unwrap();
    let err = subset_fonts(
        &xtask::repo_root(),
        &["No Such Font".into()],
        &keep,
        &scratch("unknown"),
        false,
    )
    .unwrap_err();
    assert!(err.contains("No Such Font"), "{err}");
    assert!(subset(b"not a font", &keep).is_err());
}

#[test]
fn an_emoji_subset_stays_colrv1() {
    let original =
        std::fs::read(xtask::repo_root().join("fonts/noto-color-emoji/Noto-COLRv1.ttf")).unwrap();
    let small = subset(&original, &codepoints(&["😀👍".into()], &[]).unwrap()).unwrap();
    assert!(small.len() * 100 < original.len());
    let font = FontRef::new(&small).unwrap();
    assert!(font.charmap().map('😀').is_some());
    assert!(font.charmap().map('🎉').is_none());
    assert!(font.colr().unwrap().version() >= 1);
    assert!(font.cpal().is_ok());
}
