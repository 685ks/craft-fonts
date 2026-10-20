//! `cargo xtask <command>`; run `cargo xtask help` for the list.

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "\
cargo xtask <command>

  subset [--text TEXT]... [--text-file FILE]... [--unicodes RANGES]... [--out DIR] [--checkout] FONT...
      Subset fonts for web/WASM builds. FONT is a family name from fonts/manifest.txt (every
      style), a manifest file path, or `all`. The kept characters are every character of each
      TEXT and FILE plus each RANGES list (`U+0020-U+007E,U+3000-U+30FF`). Writes DIR/<file name>
      (default DIR: target/subset); with --checkout, DIR becomes a craft-fonts checkout (fonts,
      licences and a manifest with the new checksums) to point CRAFT_FONTS_DIR at.

  help
      This text.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("subset") => subset(&args[1..]),
        Some("help" | "--help" | "-h") | None => {
            print!("{HELP}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}\n\n{HELP}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The value after flag `args[*i]`, advancing `i`.
fn value(args: &[String], i: &mut usize) -> Result<String, String> {
    let flag = &args[*i];
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn subset(args: &[String]) -> Result<(), String> {
    let mut texts = Vec::new();
    let mut ranges = Vec::new();
    let mut out = xtask::repo_root().join("target/subset");
    let mut checkout = false;
    let mut fonts = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--text" => texts.push(value(args, &mut i)?),
            "--text-file" => {
                let path = value(args, &mut i)?;
                texts.push(std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?);
            }
            "--unicodes" => ranges.push(value(args, &mut i)?),
            "--out" => out = PathBuf::from(value(args, &mut i)?),
            "--checkout" => checkout = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            font => fonts.push(font.to_string()),
        }
        i += 1;
    }
    if fonts.is_empty() {
        return Err(format!("name at least one font\n\n{HELP}"));
    }
    let cps = xtask::subset::codepoints(&texts, &ranges)?;
    if cps.is_empty() {
        return Err("nothing to keep: pass --text, --text-file or --unicodes".into());
    }
    let written = xtask::subset::subset_fonts(&xtask::repo_root(), &fonts, &cps, &out, checkout)?;
    for w in written {
        println!(
            "{}: {} -> {} bytes ({:.1}%)",
            w.path.display(),
            w.before,
            w.after,
            100.0 * w.after as f64 / w.before.max(1) as f64
        );
    }
    Ok(())
}
