use super::names::*;

// Real files of 3 by 2 pixels (5 by 7 for the last), made by ffmpeg; the point is that width and height are not the same.
const SAMPLE_JPEG: &str = "ffd8ffe000104a46494600010200000100010000fffe000f4c61766336332e312e31303200ffdb0043000804040404040505050505050606060606060606060606060607070708080807070706060707080808080909090808080809090a0a0a0c0c0b0b0e0e0e111114ffc4004c0001010000000000000000000000000000000601010100000000000000000000000000000607100100000000000000000000000000000000110100000000000000000000000000000000ffc00011080002000303012200021100031100ffda000c03010002110311003f008b004d7f7fffd9";
const SAMPLE_PNG: &str = "89504e470d0a1a0a0000000d49484452000000030000000208020000001216f14d00000009704859730000000100000001004f25c4d60000001049444154789c63fccb00052c3006030013170103814875c50000000049454e44ae426082";
const SAMPLE_GIF: &str = "47494638396103000200f71f310000002400004800006c0000900000b40000d80000fc00000024002424004824006c2400902400b42400d82400fc24000048002448004848006c4800904800b44800d84800fc4800006c00246c00486c006c6c00906c00b46c00d86c00fc6c000090002490004890006c9000909000b49000d89000fc900000b40024b40048b4006cb40090b400b4b400d8b400fcb40000d80024d80048d8006cd80090d800b4d800d8d800fcd80000fc0024fc0048fc006cfc0090fc00b4fc00d8fc00fcfc000000552400554800556c0055900055b40055d80055fc00550024552424554824556c2455902455b42455d82455fc24550048552448554848556c4855904855b44855d84855fc4855006c55246c55486c556c6c55906c55b46c55d86c55fc6c550090552490554890556c9055909055b49055d89055fc905500b45524b45548b4556cb45590b455b4b455d8b455fcb45500d85524d85548d8556cd85590d855b4d855d8d855fcd85500fc5524fc5548fc556cfc5590fc55b4fc55d8fc55fcfc550000aa2400aa4800aa6c00aa9000aab400aad800aafc00aa0024aa2424aa4824aa6c24aa9024aab424aad824aafc24aa0048aa2448aa4848aa6c48aa9048aab448aad848aafc48aa006caa246caa486caa6c6caa906caab46caad86caafc6caa0090aa2490aa4890aa6c90aa9090aab490aad890aafc90aa00b4aa24b4aa48b4aa6cb4aa90b4aab4b4aad8b4aafcb4aa00d8aa24d8aa48d8aa6cd8aa90d8aab4d8aad8d8aafcd8aa00fcaa24fcaa48fcaa6cfcaa90fcaab4fcaad8fcaafcfcaa0000ff2400ff4800ff6c00ff9000ffb400ffd800fffc00ff0024ff2424ff4824ff6c24ff9024ffb424ffd824fffc24ff0048ff2448ff4848ff6c48ff9048ffb448ffd848fffc48ff006cff246cff486cff6c6cff906cffb46cffd86cfffc6cff0090ff2490ff4890ff6c90ff9090ffb490ffd890fffc90ff00b4ff24b4ff48b4ff6cb4ff90b4ffb4b4ffd8b4fffcb4ff00d8ff24d8ff48d8ff6cd8ff90d8ffb4d8ffd8d8fffcd8ff00fcff24fcff48fcff6cfcff90fcffb4fcffd8fcfffcfcff21ff0b4e45545343415045322e30030100000021f9040404001f002c0000000003000200000806000f081c1810003b";
const SAMPLE_WEBP_LOSSLESS: &str = "524946461c000000574542505650384c0f0000002f024000000710f58ffe0722a2ff0100";
const SAMPLE_WEBP_LOSSY: &str = "524946463c000000574542505650382030000000d001009d012a0300020002003425a00274ba01f80003b000fef0e8f7ff20b96175c8d7ff203fe407fc80fff8f2000000";
const SAMPLE_WEBP_LOSSY_5X7: &str = "524946463c000000574542505650382030000000d001009d012a0500070002003425a00274ba01f80003b000fef0e8f7ff20b96175c8d7ff203fe407fc80fff8f2000000";

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

/// A deterministic stream of numbers, so a failure can be repeated.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Everything that makes a name hard: separators, dots, controls, direction marks, zero-width characters, spaces of every kind,
/// letters of other scripts, and the names Windows reserves.
const NASTY: &[&str] = &[
    "/", "\\", "..", ".", " ", "\t", "\n", "\r", "\0", "\u{202e}", "\u{200b}", "\u{feff}", "\u{00a0}", "\u{2028}", "\u{00ad}", "\u{2066}", "<", ">", ":", "\"", "|", "?", "*",
    "é", "日本語", "ß", "a", "Z", "9", "CON", "nul", ".pdf", ".exe", "COM1", " . ", "\u{1f600}",
];

fn nasty_name(rng: &mut Rng) -> String {
    (0..(rng.next() % 12) as usize).map(|_| NASTY[(rng.next() % NASTY.len() as u64) as usize]).collect()
}

#[test]
fn file_names_are_cleaned() {
    let long = "a".repeat(200);
    let cases: Vec<(String, String)> = vec![
        ("receipt.pdf".into(), "receipt.pdf".into()),
        ("/home/me/receipt.pdf".into(), "receipt.pdf".into()),
        ("C:\\Users\\me\\scan 01.jpg".into(), "scan 01.jpg".into()),
        ("../../etc/passwd".into(), "passwd".into()),
        ("folder/".into(), "document".into()),
        ("".into(), "document".into()),
        ("   ".into(), "document".into()),
        (".".into(), "document".into()),
        ("..".into(), "document".into()),
        ("...".into(), "document".into()),
        ("a\u{202e}fdp.exe".into(), "afdp.exe".into()),
        ("zero\u{200b}width.pdf".into(), "zerowidth.pdf".into()),
        ("nul\0byte.pdf".into(), "nulbyte.pdf".into()),
        ("  many   spaces\t\nhere.txt ".into(), "many spaces here.txt".into()),
        ("no\u{00a0}break\u{2028}line.txt".into(), "no break line.txt".into()),
        (".hidden".into(), ".hidden".into()),
        ("Café 日本語.pdf".into(), "Café 日本語.pdf".into()),
        (format!("{long}.pdf"), format!("{}.pdf", "a".repeat(116))),
        (long.clone(), "a".repeat(120)),
        (format!("{long}.{}", "b".repeat(30)), "a".repeat(120)),
        (format!("{}.pdf", "é".repeat(150)), format!("{}.pdf", "é".repeat(116))),
        (format!("{} .pdf", "a".repeat(118)), format!("{}.pdf", "a".repeat(116))),
    ];
    for (input, expected) in cases {
        assert_eq!(clean_file_name(&input), expected, "{input:?}");
    }
}

#[test]
fn typed_titles_keep_their_separators_as_hyphens() {
    assert_eq!(clean_title("Rent 2026/10").as_deref(), Some("Rent 2026-10"));
    assert_eq!(clean_title("a\\b").as_deref(), Some("a-b"));
    assert_eq!(clean_title("  Boiler   warranty ").as_deref(), Some("Boiler warranty"));
    assert_eq!(clean_title("x\u{202e}").as_deref(), Some("x"));
    for empty in ["", "  ", "...", "\u{200b}", "\t\n"] {
        assert_eq!(clean_title(empty), None, "{empty:?}");
    }
    // a title may be nothing but a separator, which is a hyphen
    assert_eq!(clean_title("/").as_deref(), Some("-"));
}

#[test]
fn a_clean_name_is_recognised_and_nothing_else_is() {
    for good in ["receipt.pdf", "a", ".hidden", "Café 日本語.pdf", &"a".repeat(120)] {
        assert!(is_clean_name(good), "{good:?}");
    }
    for bad in ["", " ", "..", ".", "a/b", "a\\b", " lead", "trail ", "two  spaces", "a\u{200b}b", "a\u{202e}b", "tab\t", &"a".repeat(121)] {
        assert!(!is_clean_name(bad), "{bad:?}");
    }
}

#[test]
fn cleaning_always_gives_a_clean_name_and_cleaning_twice_changes_nothing() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..3000 {
        let input = nasty_name(&mut rng);
        let once = clean_file_name(&input);
        assert!(is_clean_name(&once), "{input:?} gave {once:?}");
        assert_eq!(clean_file_name(&once), once, "{input:?}");
        assert!(once.chars().count() <= NAME_MAX_CHARS);
        if let Some(title) = clean_title(&input) {
            assert!(is_clean_name(&title), "{input:?} gave the title {title:?}");
            assert_eq!(clean_title(&title).as_deref(), Some(title.as_str()), "{input:?}");
        }
    }
}

#[test]
fn names_on_disk_are_safe_on_every_system() {
    let cases = [
        ("report.pdf", "report.pdf"),
        ("a:b*c?.pdf", "a_b_c_.pdf"),
        ("report<>.pdf", "report__.pdf"),
        ("say \"hi\".txt", "say _hi_.txt"),
        ("CON", "_CON"),
        ("con.txt", "_con.txt"),
        ("NUL.tar.gz", "_NUL.tar.gz"),
        ("COM1.pdf", "_COM1.pdf"),
        ("lpt9", "_lpt9"),
        ("AUX ", "_AUX"),
        ("COM0", "COM0"),
        ("COM10", "COM10"),
        ("CONSOLE.txt", "CONSOLE.txt"),
        ("name. . ", "name"),
        ("...", "document"),
        ("", "document"),
        ("  lead.txt", "lead.txt"),
        ("a/b\\c", "a_b_c"),
    ];
    for (input, expected) in cases {
        assert_eq!(disk_name(input), expected, "{input:?}");
    }
    let mut rng = Rng(42);
    for _ in 0..3000 {
        let name = disk_name(&nasty_name(&mut rng));
        assert!(!name.is_empty());
        assert!(!name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']) && !name.chars().any(|c| c.is_control()), "{name:?}");
        assert!(!name.ends_with(['.', ' ']) && !name.starts_with(' '), "{name:?}");
        let stem = name.split('.').next().unwrap().to_ascii_uppercase();
        assert!(!["CON", "PRN", "AUX", "NUL", "COM1", "LPT1", "COM9", "LPT9"].contains(&stem.as_str()), "{name:?}");
    }
}

#[test]
fn the_copy_given_to_a_viewer_gets_its_extension_from_the_kind_of_file() {
    let cases = [
        ("Receipt March.PDF", MediaType::Pdf, Some("Receipt March.pdf")),
        ("photo.exe", MediaType::Jpeg, Some("photo.jpg")),
        ("scan.bat", MediaType::Png, Some("scan.png")),
        ("invoice.pdf.exe", MediaType::Pdf, Some("invoice.pdf.pdf")),
        ("CON", MediaType::Text, Some("_CON.txt")),
        ("noext", MediaType::Png, Some("noext.png")),
        (".bashrc", MediaType::Text, Some(".bashrc.txt")),
        ("page.js", MediaType::Html, Some("page.html")),
        ("data.xlsx", MediaType::Csv, Some("data.csv")),
        ("run.sh", MediaType::Other, None),
        ("setup.exe", MediaType::Other, None),
    ];
    for (name, media, expected) in cases {
        assert_eq!(open_name(name, media).as_deref(), expected, "{name:?} as {media:?}");
    }
    // whatever the name says, the extension is the kind's
    let mut rng = Rng(7);
    for _ in 0..2000 {
        let name = clean_file_name(&nasty_name(&mut rng));
        for media in MediaType::ALL {
            match (open_name(&name, media), media.open_extension()) {
                (Some(opened), Some(extension)) => {
                    assert!(opened.ends_with(&format!(".{extension}")), "{name:?} gave {opened:?}");
                    assert_eq!(disk_name(&opened), opened, "{opened:?} is not safe as it is");
                }
                (None, None) => {}
                other => panic!("{name:?} as {media:?}: {other:?}"),
            }
        }
    }
}

#[test]
fn the_closed_list_of_kinds() {
    assert_eq!(MediaType::ALL.len(), 9);
    for media in MediaType::ALL {
        assert_eq!(MediaType::parse(media.as_str()), Some(media));
        assert_eq!(media.open_extension().is_none(), media == MediaType::Other);
        assert_eq!(media.is_image(), media.as_str().starts_with("image/"));
    }
    for unknown in ["", "image/svg+xml", "text/xml", "application/x-msdownload", "IMAGE/PNG", "image/png "] {
        assert_eq!(MediaType::parse(unknown), None, "{unknown:?}");
    }
}

#[test]
fn files_are_told_apart_by_their_first_bytes() {
    assert_eq!(sniff("a.jpg", &bytes(SAMPLE_JPEG)), MediaType::Jpeg);
    assert_eq!(sniff("a.png", &bytes(SAMPLE_PNG)), MediaType::Png);
    assert_eq!(sniff("a.gif", &bytes(SAMPLE_GIF)), MediaType::Gif);
    for webp in [SAMPLE_WEBP_LOSSLESS, SAMPLE_WEBP_LOSSY, SAMPLE_WEBP_LOSSY_5X7] {
        assert_eq!(sniff("a.webp", &bytes(webp)), MediaType::Webp);
    }
    assert_eq!(sniff("a.pdf", b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj"), MediaType::Pdf);
    assert_eq!(sniff("a.txt", "plain text, with Café".as_bytes()), MediaType::Text);
    assert_eq!(sniff("a.txt", b"\xef\xbb\xbfwith a byte order mark"), MediaType::Text);
    assert_eq!(sniff("a", b"no extension"), MediaType::Text);
    assert_eq!(sniff("totals.CSV", b"a,b\n1,2\n"), MediaType::Csv);
    assert_eq!(sniff("order.html", b"<html><body>Thanks</body></html>"), MediaType::Html);
    assert_eq!(sniff("order.HTM", b"<html></html>"), MediaType::Html);
    // a page without the extension of one is only text: it is opened as text, which shows its source and runs nothing
    assert_eq!(sniff("order.txt", b"<html><script>alert(1)</script></html>"), MediaType::Text);
}

#[test]
fn a_name_cannot_make_a_file_into_something_it_is_not() {
    assert_eq!(sniff("holiday.jpg", b"just some words"), MediaType::Text);
    assert_eq!(sniff("holiday.jpg", b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff"), MediaType::Other);
    assert_eq!(sniff("receipt.pdf", b"MZ\x90\x00\x00\x00 not a pdf"), MediaType::Other);
    assert_eq!(sniff("setup.exe", b"readable text"), MediaType::Text);
    assert_eq!(sniff("a.txt", b"has a \0 nul byte"), MediaType::Other);
    assert_eq!(sniff("a.txt", &[0xff, 0xfe, b'h', 0, b'i', 0]), MediaType::Other, "text in UTF-16 is not UTF-8");
    assert_eq!(sniff("a.txt", &[0x80, 0x81, 0x82]), MediaType::Other);
    // the signature of an image is not enough: its header has to be readable
    assert_eq!(sniff("a.png", &bytes(SAMPLE_PNG)[..20]), MediaType::Other);
    assert_eq!(sniff("a.jpg", &[0xff, 0xd8, 0xff, 0xe0]), MediaType::Other);
    assert_eq!(sniff("a.gif", b"GIF89a"), MediaType::Other);
    assert_eq!(sniff("a.webp", b"RIFF\x04\x00\x00\x00WEBP"), MediaType::Other);
    assert_eq!(sniff("a.png", &png_header(0, 5)), MediaType::Other, "an image with no width");
}

fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut b = bytes("89504e470d0a1a0a0000000d49484452");
    b.extend_from_slice(&width.to_be_bytes());
    b.extend_from_slice(&height.to_be_bytes());
    b.extend_from_slice(&[8, 2, 0, 0, 0]);
    b
}

fn jpeg_with(segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![0xff, 0xd8];
    for (marker, body) in segments {
        b.extend_from_slice(&[0xff, *marker]);
        b.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
        b.extend_from_slice(body);
    }
    b
}

fn frame(width: u16, height: u16) -> Vec<u8> {
    let mut body = vec![8];
    body.extend_from_slice(&height.to_be_bytes());
    body.extend_from_slice(&width.to_be_bytes());
    body.extend_from_slice(&[1, 1, 0x11, 0]);
    body
}

#[test]
fn the_size_an_image_claims_is_read_from_its_header() {
    assert_eq!(image_size(&bytes(SAMPLE_JPEG), MediaType::Jpeg), Some((3, 2)));
    assert_eq!(image_size(&bytes(SAMPLE_PNG), MediaType::Png), Some((3, 2)));
    assert_eq!(image_size(&bytes(SAMPLE_GIF), MediaType::Gif), Some((3, 2)));
    assert_eq!(image_size(&bytes(SAMPLE_WEBP_LOSSLESS), MediaType::Webp), Some((3, 2)));
    assert_eq!(image_size(&bytes(SAMPLE_WEBP_LOSSY), MediaType::Webp), Some((3, 2)));
    assert_eq!(image_size(&bytes(SAMPLE_WEBP_LOSSY_5X7), MediaType::Webp), Some((5, 7)));
    assert_eq!(image_size(&png_header(70_000, 3), MediaType::Png), Some((70_000, 3)));
    // other kinds of file have no size
    assert_eq!(image_size(b"%PDF-1.4", MediaType::Pdf), None);
    assert_eq!(image_size(&bytes(SAMPLE_PNG), MediaType::Jpeg), None, "a PNG is not read as a JPEG");
}

#[test]
fn a_jpeg_frame_is_found_after_other_segments_and_in_every_kind_of_frame() {
    // a table of Huffman codes (marker C4) and the arithmetic-coding marker (CC) sit next to the frame markers but are not frames
    let tables = vec![(0xc4, vec![0; 20]), (0xcc, vec![0; 2]), (0xe1, vec![b'x'; 300]), (0xdb, vec![0; 65])];
    for sof in [0xc0u8, 0xc1, 0xc2, 0xc3, 0xc5, 0xc9, 0xcf] {
        let mut segments = tables.clone();
        segments.push((sof, frame(640, 480)));
        assert_eq!(image_size(&jpeg_with(&segments), MediaType::Jpeg), Some((640, 480)), "marker {sof:#x}");
    }
    // the image data before any frame, or the end of the file, means there is no size
    assert_eq!(image_size(&jpeg_with(&[(0xda, vec![0; 10])]), MediaType::Jpeg), None);
    assert_eq!(image_size(&jpeg_with(&[(0xc4, vec![0; 20])]), MediaType::Jpeg), None);
    assert_eq!(image_size(&jpeg_with(&[(0xc0, frame(0, 480))]), MediaType::Jpeg), None, "no width");
    // a segment with no data is fine; one whose length field is smaller than the field itself is not a segment at all
    assert_eq!(image_size(&jpeg_with(&[(0xe0, vec![]), (0xc0, frame(10, 10))]), MediaType::Jpeg), Some((10, 10)));
    let mut broken = jpeg_with(&[(0xe0, vec![0; 10]), (0xc0, frame(10, 10))]);
    broken[5] = 1;
    assert_eq!(image_size(&broken, MediaType::Jpeg), None, "a length of 1");
}

#[test]
fn only_an_image_that_is_not_too_big_is_drawn() {
    assert!(drawable(&bytes(SAMPLE_JPEG), MediaType::Jpeg));
    assert!(drawable(&bytes(SAMPLE_PNG), MediaType::Png));
    // exactly 64 megapixels is drawn, one more row is not
    assert!(drawable(&png_header(8000, 8000), MediaType::Png));
    assert!(!drawable(&png_header(8000, 8001), MediaType::Png));
    // a header that claims the largest image there can be, in a file of 30 bytes
    assert!(!drawable(&png_header(u32::MAX, u32::MAX), MediaType::Png));
    assert!(!drawable(&jpeg_with(&[(0xc0, frame(65535, 65535))]), MediaType::Jpeg));
    assert!(drawable(&jpeg_with(&[(0xc0, frame(8000, 8000))]), MediaType::Jpeg));
    // not an image, or an image whose header cannot be read
    assert!(!drawable(b"%PDF-1.4", MediaType::Pdf));
    assert!(!drawable(&bytes(SAMPLE_PNG)[..20], MediaType::Png));
    assert!(!drawable(&bytes(SAMPLE_PNG), MediaType::Text));
}

#[test]
fn a_cut_or_garbled_header_is_never_a_panic() {
    for (sample, kind) in [
        (SAMPLE_JPEG, MediaType::Jpeg),
        (SAMPLE_PNG, MediaType::Png),
        (SAMPLE_GIF, MediaType::Gif),
        (SAMPLE_WEBP_LOSSLESS, MediaType::Webp),
        (SAMPLE_WEBP_LOSSY, MediaType::Webp),
    ] {
        let whole = bytes(sample);
        for cut in 0..whole.len().min(300) {
            let _ = image_size(&whole[..cut], kind);
            let _ = sniff("x", &whole[..cut]);
        }
        // one changed byte in the header, in every position and several values
        for at in 0..whole.len().min(60) {
            for value in [0u8, 1, 0x7f, 0x80, 0xff] {
                let mut damaged = whole.clone();
                damaged[at] = value;
                let _ = image_size(&damaged, kind);
                let _ = sniff("x", &damaged);
            }
        }
    }
    let mut rng = Rng(0xdead_beef);
    let starts: [&[u8]; 4] = [&[0xff, 0xd8, 0xff], b"\x89PNG\r\n\x1a\n", b"GIF89a", b"RIFF\x24\x00\x00\x00WEBPVP8X"];
    for _ in 0..4000 {
        let mut data = starts[(rng.next() % 4) as usize].to_vec();
        for _ in 0..(rng.next() % 80) {
            // lots of 0xff and small values, which is what makes marker scanning go wrong
            data.push(match rng.next() % 4 {
                0 => 0xff,
                1 => (rng.next() % 4) as u8,
                _ => rng.next() as u8,
            });
        }
        let kind = sniff("x", &data);
        if kind.is_image() {
            assert!(image_size(&data, kind).is_some());
        }
        for kind in [MediaType::Jpeg, MediaType::Png, MediaType::Gif, MediaType::Webp] {
            let _ = drawable(&data, kind);
        }
    }
}
