//! Assertions on the font files themselves.

use craft_fonts::{
    FONTS, Font, Role, fallback_chain, fallback_order, fallback_scripts, for_script,
};
use sha2::{Digest, Sha256};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider, string::StringId};

fn bytes(f: &Font) -> Vec<u8> {
    std::fs::read(craft_fonts_tests::repo_root().join(f.file))
        .unwrap_or_else(|e| panic!("{}: {e}", f.file))
}

#[test]
fn every_file_matches_its_manifest_checksum() {
    for f in FONTS {
        let digest = Sha256::digest(bytes(f));
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex, f.sha256,
            "{}: the file changed; update the manifest only after checking the new upstream source",
            f.file
        );
    }
}

#[test]
fn every_font_ships_its_licence() {
    for f in FONTS {
        let text = std::fs::read_to_string(craft_fonts_tests::repo_root().join(f.licence_file))
            .unwrap_or_else(|e| panic!("{}: {e}", f.licence_file));
        if f.licence == "OFL-1.1" {
            assert!(
                text.contains("SIL OPEN FONT LICENSE Version 1.1"),
                "{} is not the OFL 1.1 text",
                f.licence_file
            );
        }
    }
}

#[test]
fn every_font_is_the_family_the_manifest_says() {
    for f in FONTS {
        let data = bytes(f);
        let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let names: Vec<String> = [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]
            .into_iter()
            .flat_map(|id| {
                font.localized_strings(id)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            names.iter().any(|n| n == f.family),
            "{}: name table has {names:?}, manifest says {:?}",
            f.file,
            f.family
        );
    }
}

/// Text every Japanese font must cover: kana, common kanji, Japanese punctuation, full-width
/// forms and basic Latin (UI strings mix them).
const JAPANESE: &str = concat!(
    "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんがぎぐげござじずぜぞだぢづでどばびぶべぼぱぴぷぺぽぁぃぅぇぉっゃゅょ",
    "アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲンガギグゲゴヴァィゥェォッャュョー",
    "日本語漢字東京都新見書読学校会社電話開閉保存編集表示画像文字色線形層効果時間写真映画印刷設定終了取消削除選択複製",
    "、。「」『』・…！？（）：；〜",
    "０１２３４５６７８９ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺ",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
);

#[test]
fn japanese_fonts_cover_japanese_text() {
    let jpan: Vec<&Font> = for_script("Jpan").collect();
    assert!(!jpan.is_empty(), "the manifest lists no Japanese font");
    for f in jpan {
        let data = bytes(f);
        let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let charmap = font.charmap();
        let missing: String = JAPANESE
            .chars()
            .filter(|c| charmap.map(*c).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "{} {} has no glyph for {missing:?}",
            f.family,
            f.style
        );
    }
}

#[test]
fn japanese_glyphs_have_outlines() {
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::{DrawSettings, OutlinePen};
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
    for f in for_script("Jpan") {
        let data = bytes(f);
        let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let outlines = font.outline_glyphs();
        for c in ['漢', 'あ', 'ア', '。'] {
            let gid = font
                .charmap()
                .map(c)
                .unwrap_or_else(|| panic!("{}: no glyph for {c}", f.file));
            let glyph = outlines
                .get(gid)
                .unwrap_or_else(|| panic!("{}: no outline for {c}", f.file));
            let mut pen = Count(0);
            glyph
                .draw(
                    DrawSettings::unhinted(Size::new(64.0), LocationRef::default()),
                    &mut pen,
                )
                .unwrap_or_else(|e| panic!("{}: drawing {c}: {e}", f.file));
            assert!(pen.0 > 0, "{} {}: {c} draws nothing", f.family, f.style);
        }
    }
}

#[test]
fn japanese_fonts_support_vertical_text() {
    // Vertical type (tategaki) needs the vertical alternates for punctuation and long vowels.
    for f in for_script("Jpan") {
        let data = bytes(f);
        let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let gsub = font
            .gsub()
            .unwrap_or_else(|e| panic!("{}: no GSUB: {e}", f.file));
        let features = gsub
            .feature_list()
            .unwrap_or_else(|e| panic!("{}: GSUB features: {e}", f.file));
        let tags: Vec<String> = features
            .feature_records()
            .iter()
            .map(|r| r.feature_tag().to_string())
            .collect();
        assert!(
            tags.iter().any(|t| t == "vert" || t == "vrt2"),
            "{} {}: no vert/vrt2 GSUB feature (has {tags:?})",
            f.family,
            f.style
        );
    }
}

/// Coverage of `text` by every font tagged with `script`.
fn covers(script: &str, text: &str) {
    let fonts: Vec<&Font> = for_script(script).collect();
    assert!(!fonts.is_empty(), "the manifest lists no {script} font");
    for f in fonts {
        let data = bytes(f);
        let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let charmap = font.charmap();
        let missing: String = text.chars().filter(|c| charmap.map(*c).is_none()).collect();
        assert!(
            missing.is_empty(),
            "{} {} has no glyph for {missing:?}",
            f.family,
            f.style
        );
    }
}

#[test]
fn simplified_chinese_fonts_cover_chinese_text() {
    covers(
        "Hans",
        concat!(
            "中文简体字文件编辑视图窗口帮助新建打开保存关闭撤销重做复制粘贴删除选择图层页面样式颜色导出打印设置",
            "，。、：；？！（）《》“”…",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
        ),
    );
}

#[test]
fn arabic_fonts_cover_arabic_text() {
    covers(
        "Arab",
        concat!(
            "ابتثجحخدذرزسشصضطظعغفقكلمنهوي",
            "ءآأؤإئةى",
            "٠١٢٣٤٥٦٧٨٩",
            "،؛؟",
        ),
    );
}

#[test]
fn thai_fonts_cover_thai_text() {
    covers(
        "Thai",
        concat!(
            "กขฃคฅฆงจฉชซฌญฎฏฐฑฒณดตถทธนบปผฝพฟภมยรฤลฦวศษสหฬอฮ",
            "ะัาำิีึืุูเแโใไๅ็่้๊๋์",
            "๐๑๒๓๔๕๖๗๘๙",
            "ฯๆ฿",
        ),
    );
}

#[test]
fn georgian_fonts_cover_georgian_text() {
    covers(
        "Geor",
        concat!(
            "აბგდევზთიკლმნოპჟრსტუფქღყშჩცძწჭხჯჰ",
            "ᲐᲑᲒᲓᲔᲕᲖᲗᲘᲙᲚᲛᲜᲝᲞᲟᲠᲡᲢᲣᲤᲥᲦᲧᲨᲩᲪᲫᲬᲭᲮᲯᲰ",
            "„“–№",
        ),
    );
}

#[test]
fn latin_ui_fonts_cover_latin_greek_and_cyrillic_text() {
    covers(
        "Latn",
        concat!(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            "àáâãäåæçèéêëìíîïñòóôõöøùúûüýÿßœŒ€£¥©®™°±×÷",
            ".,:;!?¡¿'\"()[]{}<>/\\|@#$%&*+-=_~^`…–—‘’“”•→←",
        ),
    );
    covers("Grek", "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩαβγδεζηθικλμνξοπρστυφχψω");
    covers(
        "Cyrl",
        "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя",
    );
}

#[test]
fn the_house_ui_font_has_tabular_figures() {
    // The design system sets every number in tabular figures (`font-variant-numeric: tabular-nums`).
    let inter = FONTS
        .iter()
        .find(|f| f.family == "Inter")
        .expect("the manifest lists Inter, the house UI font");
    let data = bytes(inter);
    let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", inter.file));
    let gsub = font
        .gsub()
        .unwrap_or_else(|e| panic!("{}: no GSUB: {e}", inter.file));
    let features = gsub
        .feature_list()
        .unwrap_or_else(|e| panic!("{}: GSUB features: {e}", inter.file));
    assert!(
        features
            .feature_records()
            .iter()
            .any(|r| r.feature_tag() == skrifa::Tag::new(b"tnum")),
        "Inter has no tnum feature"
    );
}

#[test]
fn the_monospace_font_has_equal_advances() {
    let mono = FONTS
        .iter()
        .find(|f| f.family == "JetBrains Mono")
        .expect("the manifest lists JetBrains Mono");
    let data = bytes(mono);
    let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", mono.file));
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    let advances: Vec<f32> = "iWm0.:"
        .chars()
        .map(|c| {
            let gid = charmap.map(c).unwrap_or_else(|| panic!("no glyph for {c}"));
            metrics.advance_width(gid).unwrap_or_default()
        })
        .collect();
    assert!(
        advances.windows(2).all(|w| w[0] == w[1]),
        "advances differ: {advances:?}"
    );
}

#[test]
fn korean_fonts_cover_korean_text() {
    covers(
        "Kore",
        concat!(
            "가나다라마바사아자차카타파하한국어글꼴파일편집보기창도움말새로열기저장닫기실행취소복사붙여넣기삭제선택",
            "ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎㅏㅑㅓㅕㅗㅛㅜㅠㅡㅣ",
            "韓國漢字",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
        ),
    );
}

#[test]
fn traditional_chinese_fonts_cover_chinese_text() {
    covers(
        "Hant",
        concat!(
            "中文繁體字檔案編輯檢視視窗說明新增開啟儲存關閉復原重做複製貼上刪除選擇圖層頁面樣式顏色匯出列印設定",
            "，。、：；？！（）「」『』…",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
        ),
    );
}

#[test]
fn hebrew_fonts_cover_hebrew_text() {
    covers(
        "Hebr",
        concat!(
            "אבגדהוזחטיכךלמםנןסעפףצץקרשת",
            "\u{05B0}\u{05B4}\u{05B7}\u{05B8}\u{05BC}\u{05C1}\u{05C2}",
            "׳״₪",
        ),
    );
}

#[test]
fn devanagari_fonts_cover_devanagari_text() {
    covers(
        "Deva",
        concat!(
            "अआइईउऊऋएऐओऔकखगघङचछजझञटठडढणतथदधनपफबभमयरलवशषसह",
            "\u{093E}\u{093F}\u{0940}\u{0941}\u{0942}\u{0947}\u{0948}\u{094B}\u{094C}\u{094D}\u{0902}\u{0903}\u{093C}",
            "०१२३४५६७८९।॥",
        ),
    );
}

#[test]
fn the_emoji_font_covers_emoji_in_colrv1() {
    covers("Zsye", "😀😂🥲👍🎉❤🔥✅🚀🌍🍣🐱");
    for f in for_script("Zsye") {
        let data = bytes(f);
        let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let colr = font
            .colr()
            .unwrap_or_else(|e| panic!("{}: no COLR table: {e}", f.file));
        assert!(
            colr.version() >= 1,
            "{}: COLR version {} (want COLRv1)",
            f.file,
            colr.version()
        );
        assert!(font.cpal().is_ok(), "{}: no CPAL palette", f.file);
    }
}

/// A short sample of each script that has a fallback chain.
const SAMPLES: &[(&str, &str)] = &[
    ("Latn", "Hello, wörld! ÀÉÎõü"),
    ("Grek", "Καλημέρα κόσμε"),
    ("Cyrl", "Привет, мир"),
    ("Jpan", "日本語のテキスト、カタカナ。"),
    ("Hans", "简体中文文本"),
    ("Hant", "繁體中文文本"),
    ("Kore", "한국어 텍스트"),
    ("Hang", "한글"),
    ("Arab", "النص العربي"),
    ("Hebr", "טקסט בעברית"),
    ("Deva", "हिन्दी पाठ"),
    ("Thai", "ข้อความภาษาไทย"),
    ("Geor", "ქართული ტექსტი"),
    ("Zsye", "😀👍🎉"),
];

#[test]
fn every_fallback_chain_starts_with_a_font_that_covers_its_script() {
    for role in Role::ALL {
        let scripts = fallback_scripts(role);
        for script in &scripts {
            let sample = SAMPLES
                .iter()
                .find(|(s, _)| s == script)
                .unwrap_or_else(|| panic!("add a {script} sample to SAMPLES"))
                .1;
            let head = fallback_chain(role, script)[0];
            let data = bytes(head);
            let font = FontRef::new(&data).unwrap_or_else(|e| panic!("{}: {e}", head.file));
            let charmap = font.charmap();
            let missing: String = sample
                .chars()
                .filter(|c| !c.is_whitespace() && charmap.map(*c).is_none())
                .collect();
            assert!(
                missing.is_empty(),
                "{role:?} {script}: {} has no glyph for {missing:?}",
                head.family
            );
        }
        for (script, _) in SAMPLES {
            assert!(scripts.contains(script), "{role:?} has no {script} chain");
        }
    }
}

#[test]
fn the_fallback_chains_put_the_house_fonts_first_and_emoji_last() {
    let first = |role, script| fallback_chain(role, script)[0].family;
    assert_eq!(first(Role::Ui, "Latn"), "Inter");
    assert_eq!(first(Role::Document, "Latn"), "Inter");
    assert_eq!(first(Role::Mono, "Latn"), "JetBrains Mono");
    assert_eq!(first(Role::Ui, "Jpan"), "BIZ UDPGothic");
    assert_eq!(first(Role::Document, "Jpan"), "Shippori Mincho");
    // No monospace CJK font: the mono chain borrows the UI one.
    assert_eq!(first(Role::Mono, "Hans"), "Noto Sans CJK SC");
    for role in Role::ALL {
        let order = fallback_order(role);
        let want = if role == Role::Mono {
            "JetBrains Mono"
        } else {
            "Inter"
        };
        assert_eq!(order[0].family, want, "{role:?}");
        assert_eq!(
            order[order.len() - 1].family,
            "Noto Color Emoji",
            "{role:?}"
        );
        // Japanese before Chinese and Korean, so shared Han characters get Japanese forms.
        let pos = |family: &str| order.iter().position(|f| f.family == family).unwrap();
        let jpan = if role == Role::Document {
            "Shippori Mincho"
        } else {
            "BIZ UDPGothic"
        };
        assert!(pos(jpan) < pos("Noto Sans CJK SC"), "{role:?}");
        assert!(pos("Noto Sans CJK SC") < pos("Noto Sans KR"), "{role:?}");
        if role == Role::Mono {
            assert!(!order.iter().any(|f| f.family == "Inter"), "Inter in mono");
        }
    }
}
