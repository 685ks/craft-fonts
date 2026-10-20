craft-fonts
===========

Reusable fonts for the Crafting Apps.

This library is meant to have the following:

1. Reusable font assets. These may be large files, and since we're using a separate repo for storing them,
   We're not afraid of potentially having to do a git reset to purge stale assets. It won't impact the other
   repos with core app and library functionality.

2. Additional cross-cutting Rust libraries for dealing with fonts. It's uncertain whether it's better to
   have code packages as individual repos as one classically does, or whether we should take a
   more "biology-inspired" approach of copying innovations between repos as a form of lateral code/information
   exchange.

## What's here

| Path | What |
|---|---|
| `fonts/` | The font files, one directory per family, each with its licence. |
| `fonts/manifest.txt` | Machine-readable list of every font: family, style, file, scripts, licence, SHA-256, pinned source, fallback-chain ranks. |
| `ATTRIBUTION.md` | Authors, pinned sources and licences. |
| `crates/craft-fonts` | Rust: the manifest as a static table, finding a checkout (`CRAFT_FONTS_DIR`), loading it, and an `embed` feature that compiles the fonts in. |
| `crates/craft-fonts-build` | Rust, no dependencies: the build-script helper apps use (as a build-dependency only) to embed an optional checkout's fonts with their fallback chains; also defines the manifest format. |
| `crates/craft-fonts-tests` | Tests asserted on these fonts: checksums, licences, family names, per-script coverage, Japanese outlines and vertical-text (`vert`/`vrt2`) support, COLRv1 emoji. |
| `crates/xtask` | Repository tools: `cargo xtask subset` (web/WASM subsets) and `cargo xtask fetch` (pinned upstream downloads). |
| `docs/integration.md` | How an app uses craft-fonts as a fully optional build input. |

### Fonts

| Family | Styles | For | Licence |
|---|---|---|---|
| BIZ UDPGothic | Regular, Bold | Japanese UI text (sans) | OFL-1.1 |
| Shippori Mincho | Regular | Japanese document text (serif) | OFL-1.1 |
| BIZ UDMincho | Regular | Japanese document text (serif) | OFL-1.1 |
| Noto Sans CJK SC | Regular | Simplified Chinese UI and document text (sans) | OFL-1.1 |
| Noto Sans Arabic | Regular (variable: wdth, wght) | Arabic UI and document text (sans) | OFL-1.1 |
| Noto Sans Thai | Regular (variable: wdth, wght) | Thai UI and document text (sans) | OFL-1.1 |
| Noto Sans Georgian | Regular (variable: wdth, wght) | Georgian UI and document text (sans) | OFL-1.1 |
| Inter | Regular (variable: opsz, wght) | The house UI font for Latin, Greek and Cyrillic (has tabular figures) | OFL-1.1 |
| JetBrains Mono | Regular (variable: wght) | Monospace UI text: command lines, code, numeric columns | OFL-1.1 |
| Noto Sans KR | Regular (variable: wght) | Korean UI and document text (sans) | OFL-1.1 |
| Noto Sans TC | Regular (variable: wght) | Traditional Chinese UI and document text (sans) | OFL-1.1 |
| Noto Sans Hebrew | Regular (variable: wdth, wght) | Hebrew UI and document text (sans) | OFL-1.1 |
| Noto Sans Devanagari | Regular (variable: wdth, wght) | Devanagari (Hindi, Marathi, Nepali…) UI and document text (sans) | OFL-1.1 |
| Noto Color Emoji | Regular (COLRv1) | Colour emoji (vector COLRv1; needs a COLRv1-capable renderer) | OFL-1.1 |

### Fallback chains

Each manifest line can rank its font in the per-script fallback chains for three roles: `ui`
(interface text), `doc` (document text) and `mono` (monospace; a script with no `mono` entry uses
its `ui` chain). Priorities are global, lower first: Latin/Greek/Cyrillic (10: Inter, or JetBrains
Mono for `mono`), Japanese (20–22), Simplified/Traditional Chinese and Korean (23–25), Arabic,
Hebrew, Devanagari, Thai, Georgian (30–34), emoji (90). Japanese comes before Chinese and Korean
so Han characters shared between them get Japanese forms by default.

```rust
use craft_fonts::{Role, fallback_chain, fallback_order};
let korean_ui = fallback_chain(Role::Ui, "Kore");   // [Noto Sans KR]
let document = fallback_order(Role::Document);      // Inter, Shippori Mincho, …, Noto Color Emoji
```

## How the apps use it

**Fully optional, never required.** The apps don't depend on the fonts: building with
`CRAFT_FONTS_DIR=<path to this checkout>` makes an app embed the fonts in `fonts/manifest.txt`;
without it, the app builds and runs as before. Release builds check out a pinned commit of this
repo and set the option. The embedding is done by the `craft-fonts-build` helper, the only thing an
app lists (under `[build-dependencies]`):

```toml
[build-dependencies]
craft-fonts-build = { git = "https://github.com/685ks/craft-fonts", rev = "<full commit SHA>" }
```

```rust
// build.rs
fn main() {
    craft_fonts_build::Builder::new()
        .web_fonts(&[("BIZ UDPGothic", "Regular")]) // wasm32: only these (the default)
        .emit();
}

// in the crate
include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));
// CRAFT_FONTS, craft_font_chain(CraftFontRole::Ui, "Kore"), craft_font_order(CraftFontRole::Mono), …
```

`Builder::legacy(true)` produces exactly what the old copied `build.rs` recipe did, so an app can
switch without touching its own `CraftFont` struct. See [`docs/integration.md`](docs/integration.md)
for every option, how to use the chains in egui and document engines, and releases.

**Font assets for the Crafting Apps belong here**, not in the app repos: add the file, its licence,
a manifest line and an attribution row, and the tests check it.

## Subsetting for web/WASM builds

Web hosts limit file sizes and every byte is downloaded, so a web build should embed only the
characters it shows. `cargo xtask subset` cuts fonts down with [skera](https://crates.io/crates/skera)
(the Rust port of HarfBuzz's `hb-subset`; layout rules, variations and COLRv1 are kept):

```sh
# Inter and BIZ UDPGothic cut to ASCII, kana and the app's own strings, as a checkout of their own:
cargo xtask subset --checkout --out target/web-fonts \
    --unicodes U+0020-U+007E,U+3000-U+30FF --text-file ../my-app/assets/strings.txt \
    Inter "BIZ UDPGothic"
CRAFT_FONTS_DIR=$PWD/target/web-fonts cargo build --target wasm32-unknown-unknown  # in the app
```

`FONT` is a family name (every style), a manifest file path or `all`. Without `--checkout` the
subset files are written flat into `--out` (default `target/subset`). With it, the output has the
same layout as this repo, the licences and a `fonts/manifest.txt` with the new checksums, so the
`craft-fonts-build` helper reads it like a full checkout (remember `Builder::web_fonts`).

## Develop

```sh
cargo test --workspace                       # all font assertions
cargo test -p craft-fonts --features embed   # the embedded bytes match the files
```

Code is MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`); each font keeps its own licence.
