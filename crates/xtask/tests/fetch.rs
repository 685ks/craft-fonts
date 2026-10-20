//! `cargo xtask fetch` and the pins in `fonts/upstream.txt`, offline.

use std::path::Path;

use xtask::fetch::{Outcome, Pin, curl, fetch, parse_pins, read_pins, verify};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-fetch-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn every_manifest_file_is_pinned_with_the_same_checksum() {
    let root = xtask::repo_root();
    let pins = read_pins(&root).unwrap();
    let text = std::fs::read_to_string(root.join("fonts/manifest.txt")).unwrap();
    for e in craft_fonts_build::parse_manifest(&text).unwrap() {
        let pin = pins
            .iter()
            .find(|p| p.file == e.file)
            .unwrap_or_else(|| panic!("{} is not in fonts/upstream.txt", e.file));
        assert_eq!(pin.sha256, e.sha256, "{}", e.file);
        assert!(
            pins.iter().any(|p| p.file == e.licence_file),
            "{} is not in fonts/upstream.txt",
            e.licence_file
        );
    }
    for p in &pins {
        assert!(p.url.starts_with("https://"), "{}: {}", p.file, p.url);
    }
}

#[test]
fn the_checked_in_files_match_their_pins() {
    let root = xtask::repo_root();
    let bad: Vec<_> = verify(&read_pins(&root).unwrap(), &root)
        .into_iter()
        .filter(|(_, o)| *o != Outcome::Present)
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn malformed_pins_are_errors() {
    let sha = "0".repeat(64);
    assert!(parse_pins("# c\n\n").unwrap().is_empty());
    assert!(parse_pins("a | b").is_err());
    assert!(parse_pins(&format!("a | {} | https://x", "Z".repeat(64))).is_err());
    assert!(parse_pins(&format!("../a | {sha} | https://x")).is_err());
    assert!(parse_pins(&format!("a | {sha} | http://x")).is_err());
    assert_eq!(
        parse_pins(&format!("fonts/a.ttf | {sha} | https://x/a.ttf")).unwrap(),
        vec![Pin {
            file: "fonts/a.ttf".into(),
            sha256: sha,
            url: "https://x/a.ttf".into()
        }]
    );
}

/// The Hebrew font and its licence, pinned to `file://` URLs of this checkout.
fn local_pins() -> Vec<Pin> {
    let root = xtask::repo_root();
    read_pins(&root)
        .unwrap()
        .into_iter()
        .filter(|p| p.file.starts_with("fonts/noto-sans-hebrew/"))
        .map(|p| Pin {
            url: format!("file://{}", root.join(&p.file).display()),
            ..p
        })
        .collect()
}

#[test]
fn fetching_downloads_verifies_and_then_skips() {
    let dest = scratch("ok");
    let pins = local_pins();
    assert_eq!(pins.len(), 2);
    let first = fetch(&pins, &dest, false, &curl).unwrap();
    assert!(
        first.iter().all(|(_, o)| *o == Outcome::Fetched),
        "{first:?}"
    );
    let again = fetch(&pins, &dest, false, &curl).unwrap();
    assert!(
        again.iter().all(|(_, o)| *o == Outcome::Present),
        "{again:?}"
    );
    assert!(
        verify(&pins, &dest)
            .iter()
            .all(|(_, o)| *o == Outcome::Present)
    );
    std::fs::remove_dir_all(dest).unwrap();
}

#[test]
fn a_download_with_the_wrong_checksum_is_rejected_and_not_written() {
    let dest = scratch("bad");
    let pins = local_pins();
    let tampered =
        |_: &str, to: &Path| std::fs::write(to, b"not the font").map_err(|e| e.to_string());
    let err = fetch(&pins, &dest, false, &tampered).unwrap_err();
    assert!(err.contains("pinned"), "{err}");
    let font = dest.join(&pins[0].file);
    assert!(!font.exists(), "a bad download was written");
    let leftovers: Vec<_> = std::fs::read_dir(font.parent().unwrap()).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(matches!(verify(&pins, &dest)[0].1, Outcome::Bad(_)));
    std::fs::remove_dir_all(dest).unwrap();
}
