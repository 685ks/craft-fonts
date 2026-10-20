# Using craft-fonts in a Crafting App

craft-fonts is a **fully optional** source of fonts. An app must build, test and run without it;
with it, the app embeds the fonts listed in `fonts/manifest.txt` and uses them (today: Japanese
UI and document fallback).

**The house UI font.** Inter is the design system's only family, and JetBrains Mono is its
monospace companion. An app using the house theme (`craft-ui`'s `house` module) makes Inter the
first entry of egui's proportional family and JetBrains Mono the first entry of the monospace
family, ahead of its own bundled fonts; the script fonts above stay as fallbacks. Both are
variable fonts: egui draws their default instance (Regular), so an app that needs real Medium,
SemiBold or Bold weights should instance them at build time rather than synthesise bold.

## Why only a build-dependency, and only a tiny one

Cargo resolves every dependency in `Cargo.toml`, optional ones included, even when the feature
that enables them is off. So apps never depend on the `craft-fonts` crate or on the font files: the
fonts come in through an environment variable at build time. The one thing an app may depend on is
the **`craft-fonts-build`** helper (`crates/craft-fonts-build`), as a build-dependency: it has no
dependencies of its own and works the same whether or not `CRAFT_FONTS_DIR` is set.

## The build option: `CRAFT_FONTS_DIR`

```sh
git clone https://github.com/storytold/craft-fonts ../craft-fonts
CRAFT_FONTS_DIR=$PWD/../craft-fonts cargo build --release
```

- **Unset** (the default): nothing changes. The app uses its own bundled fonts and system fonts.
- **Set to a craft-fonts checkout:** the app's `build.rs` reads `fonts/manifest.txt` and embeds
  every font it lists.
- **Set but not a checkout** (no manifest, missing file): the build prints a `cargo::warning` and
  carries on without craft-fonts, unless `CRAFT_FONTS_REQUIRED=1` is also set, which turns it
  into a build error. Release workflows set both, so a release can't silently ship without them.

Use an **absolute** path (e.g. `CRAFT_FONTS_DIR=$PWD/../craft-fonts`): Cargo runs build scripts in
the crate's own directory, so a relative path would resolve from there. `CRAFT_FONTS_DIR` is read by
`build.rs`, so changing it rebuilds only the crate that embeds the fonts.

## The build helper: `craft-fonts-build`

In the one crate that installs fonts (usually the UI or text crate):

```toml
[build-dependencies]
# Pin a commit and bump it deliberately, like the release checkout below. Cargo clones this
# repository (fonts included) into ~/.cargo/git once; to avoid that, vendor the two files of
# crates/craft-fonts-build into the app's workspace and use a `path` dependency instead.
craft-fonts-build = { git = "https://github.com/685ks/craft-fonts", rev = "<full commit SHA>" }
```

```rust
// build.rs
fn main() {
    craft_fonts_build::Builder::new().emit();
}
```

```rust
// src/fonts.rs (or wherever the fonts are installed)
include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));
```

That defines, in the including module:

| Item | What |
|---|---|
| `CRAFT_FONTS: &[CraftFont]` | Every embedded font: `family`, `style`, `scripts`, `fallback`, `licence`, `licence_text`, `bytes`. Empty without `CRAFT_FONTS_DIR`. |
| `CraftFontRole` | `Ui`, `Document`, `Mono`. |
| `craft_font_chain(role, script)` | The embedded fonts to try, in order, for one ISO 15924 script (`"Kore"`, `"Arab"`, …). `Mono` borrows the `Ui` chain for scripts without a monospace font. |
| `craft_font_order(role)` | Every embedded font for the role in one fallback order (what an egui font family wants). |
| `craft_font_scripts(role)` | The scripts that have a chain. |

The options on `Builder` (all optional):

- `.web_fonts(&[("BIZ UDPGothic", "Regular"), ("Inter", "Regular")])`: the fonts embedded on
  `wasm32` (default: `BIZ UDPGothic Regular` only, as before). Web hosts limit file size (e.g.
  Cloudflare's 25 MiB); see the `subset` xtask in the README for smaller web fonts.
- `.include(|e| …)`: embed only some entries on every target, e.g. only `Jpan` fonts. The whole
  manifest is ~71 MB, so an app that does not need every script should filter.
- `.legacy(true)`: emit only `CraftFont { family, style, scripts, bytes }` initialisers, for an app
  that still defines `CraftFont` itself as the old copied recipe did. A drop-in replacement for it.
- `.dir(path)` / `.no_checkout()` / `.required(bool)` / `.out_file(name)`: override
  `CRAFT_FONTS_DIR`, `CRAFT_FONTS_REQUIRED` and the output name (`craft_fonts.rs`).

Errors never panic: a bad checkout is a `cargo::warning` and an empty list, or a `cargo::error`
when `CRAFT_FONTS_REQUIRED` is set. `CRAFT_FONTS` is always defined, so there is no `cfg` to thread
through: an empty slice means the app was built without craft-fonts. Code that uses it must work
when it is empty. `crates/craft-fonts-tests/build.rs` runs the helper the way an app does.

## Using the fonts

- **UI (egui):** append `craft_font_order(CraftFontRole::Ui)` (proportional) and
  `craft_font_order(CraftFontRole::Mono)` (monospace) as fallbacks at the end of each font family,
  after the app's own fonts and before system-font discovery, so every script renders the same
  everywhere the app was built with craft-fonts. Japanese is ranked before Chinese and Korean, so
  shared Han characters get Japanese forms.
- **Document text engines:** for a run in script `S`, try the document's requested font, then the
  app's bundled fonts, then `craft_font_chain(CraftFontRole::Document, S)`.
- **Emoji:** Noto Color Emoji is COLRv1 (vector colour layers); it is last in every order. A
  renderer without COLRv1 support draws nothing useful from it, so skip it there.
- **Web (wasm32):** the helper embeds only `BIZ UDPGothic Regular` (~4.5 MB); all the fonts are
  ~71 MB, over typical per-file hosting limits. If the app's web build has a size check, measure
  with `CRAFT_FONTS_DIR` set and shrink `WEB_FONTS` (even to empty) if it fails.
- The chains encode the house choices: `BIZ UDPGothic` for Japanese UI text, `Shippori Mincho` /
  `BIZ UDMincho` for Japanese document text, Inter / JetBrains Mono for Latin, Greek and Cyrillic.
- Tests that assert on these fonts' glyphs must skip (not fail) when `CRAFT_FONTS` is empty, and
  CI should run them with `CRAFT_FONTS_DIR` set so they are exercised.

## Releases and licences

The fonts are OFL-1.1: they can be embedded and redistributed, but the licence text must go with
them. In `release.yml`, check out craft-fonts at a pinned commit and build with it:

```yaml
- uses: actions/checkout@v4
  with:
    repository: storytold/craft-fonts
    ref: <full commit SHA>        # bump deliberately
    path: craft-fonts
# then, on the build steps:
env:
  CRAFT_FONTS_DIR: ${{ github.workspace }}/craft-fonts
  CRAFT_FONTS_REQUIRED: "1"
```

Packages built with craft-fonts must include each embedded font's licence file (copy
`$CRAFT_FONTS_DIR/fonts/*/OFL.txt` into the package's licences, named after the family; `CraftFont::licence_text` also has it), and the
app's `NOTICE` must say that builds made with `CRAFT_FONTS_DIR` embed the fonts listed in
craft-fonts' `ATTRIBUTION.md`.

## Adding a font

1. Put the file and its licence under `fonts/<family>/`, from a pinned upstream commit.
2. Add a manifest line (with its SHA-256 and its fallback ranks), `fonts/upstream.txt` lines for the
   font and its licence (commit-pinned download URLs), and an `ATTRIBUTION.md` row. Check the pins
   with `cargo xtask fetch --force --dest /tmp/cf` (downloads and verifies every file).
3. `cargo test --workspace`: `craft-fonts-tests` checks the checksum, licence, family name and,
   for Japanese fonts, coverage and vertical-text support. Add assertions for any new script.
