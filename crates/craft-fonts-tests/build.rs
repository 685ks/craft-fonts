//! Runs the `craft-fonts-build` helper the way an app's `build.rs` does, against this checkout,
//! so `tests/build_helper.rs` can check the generated code compiles and agrees with the manifest.
//! Only a few small fonts are embedded to keep the test binary small.

fn small(e: &craft_fonts_build::Entry) -> bool {
    ["Noto Sans Hebrew", "Noto Sans Georgian", "JetBrains Mono"].contains(&e.family.as_str())
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    craft_fonts_build::Builder::new()
        .dir(&root)
        .required(true)
        .include(small)
        .emit();
    craft_fonts_build::Builder::new()
        .dir(&root)
        .required(true)
        .include(small)
        .legacy(true)
        .out_file("craft_fonts_legacy.rs")
        .emit();
    craft_fonts_build::Builder::new()
        .no_checkout()
        .out_file("craft_fonts_none.rs")
        .emit();
}
