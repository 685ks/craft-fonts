//! The code `craft-fonts-build` generates (see `build.rs`) compiles and matches the manifest.

mod generated {
    include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));
}

mod legacy {
    /// The struct apps defined themselves with the old copied recipe.
    pub struct CraftFont {
        pub family: &'static str,
        pub style: &'static str,
        pub scripts: &'static [&'static str],
        pub bytes: &'static [u8],
    }
    include!(concat!(env!("OUT_DIR"), "/craft_fonts_legacy.rs"));
}

mod none {
    include!(concat!(env!("OUT_DIR"), "/craft_fonts_none.rs"));
}

use generated::{
    CRAFT_FONTS, CraftFontRole, craft_font_chain, craft_font_order, craft_font_scripts,
};

fn role(r: CraftFontRole) -> craft_fonts::Role {
    match r {
        CraftFontRole::Ui => craft_fonts::Role::Ui,
        CraftFontRole::Document => craft_fonts::Role::Document,
        CraftFontRole::Mono => craft_fonts::Role::Mono,
    }
}

#[test]
fn the_selected_fonts_are_embedded_with_their_licences() {
    let families: Vec<&str> = CRAFT_FONTS.iter().map(|f| f.family).collect();
    assert_eq!(
        families,
        ["Noto Sans Georgian", "JetBrains Mono", "Noto Sans Hebrew"]
    );
    for f in CRAFT_FONTS {
        let m = craft_fonts::FONTS
            .iter()
            .find(|m| m.family == f.family && m.style == f.style)
            .unwrap();
        let file = std::fs::read(craft_fonts_tests::repo_root().join(m.file)).unwrap();
        assert_eq!(f.bytes, file.as_slice(), "{}", f.family);
        assert_eq!(f.scripts, m.scripts);
        assert_eq!(f.licence, "OFL-1.1");
        assert!(f.licence_text.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert_eq!(f.fallback.len(), m.fallback.len());
    }
}

#[test]
fn the_generated_chains_agree_with_the_crate_on_the_embedded_fonts() {
    for r in [
        CraftFontRole::Ui,
        CraftFontRole::Document,
        CraftFontRole::Mono,
    ] {
        for script in craft_font_scripts(r) {
            let got: Vec<&str> = craft_font_chain(r, script).map(|f| f.family).collect();
            let want: Vec<&str> = craft_fonts::fallback_chain(role(r), script)
                .into_iter()
                .map(|f| f.family)
                .filter(|f| CRAFT_FONTS.iter().any(|c| c.family == *f))
                .collect();
            assert_eq!(got, want, "{r:?} {script}");
        }
        let got: Vec<&str> = craft_font_order(r).map(|f| f.family).collect();
        let want: Vec<&str> = craft_fonts::fallback_order(role(r))
            .into_iter()
            .map(|f| f.family)
            .filter(|f| CRAFT_FONTS.iter().any(|c| c.family == *f))
            .collect();
        assert_eq!(got, want, "{r:?}");
    }
    let hebrew: Vec<&str> = craft_font_chain(CraftFontRole::Ui, "Hebr")
        .map(|f| f.family)
        .collect();
    assert_eq!(hebrew, ["Noto Sans Hebrew"]);
    let mono: Vec<&str> = craft_font_order(CraftFontRole::Mono)
        .map(|f| f.family)
        .collect();
    assert_eq!(mono[0], "JetBrains Mono");
}

#[test]
fn legacy_and_empty_outputs_compile() {
    let legacy: Vec<(&str, &str, usize)> = legacy::CRAFT_FONTS
        .iter()
        .map(|f| (f.family, f.style, f.scripts.len()))
        .collect();
    assert_eq!(legacy.len(), 3);
    assert!(legacy::CRAFT_FONTS.iter().all(|f| !f.bytes.is_empty()));
    assert!(none::CRAFT_FONTS.is_empty());
    assert_eq!(
        none::craft_font_chain(none::CraftFontRole::Ui, "Latn").count(),
        0
    );
}
