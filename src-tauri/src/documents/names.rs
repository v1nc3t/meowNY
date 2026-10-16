//! What a document is called and what kind of file it is (`docs/documents-design.md`, section 6).
//!
//! A file is untrusted: its name can hide characters or reach out of a folder, its extension can lie about what it holds, and an
//! image can be a few bytes that claim to be gigabytes wide. Nothing here opens, runs or renders a file; it only looks at the
//! first bytes and at the name.

/// The longest name a document has, in characters.
pub const NAME_MAX_CHARS: usize = 120;
/// An extension longer than this (with its dot) is not kept when a long name is cut.
const EXTENSION_KEEP_CHARS: usize = 12;
/// An image is drawn in the app only when it has at most this many pixels (64 megapixels). A bigger one is opened by the system's
/// viewer or saved, so a small file cannot make the app allocate gigabytes.
pub const MAX_IMAGE_PIXELS: u64 = 64_000_000;

/// The kinds of file the app tells apart. The list is closed: the database refuses any other value, and so does a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Jpeg,
    Png,
    Gif,
    Webp,
    Pdf,
    Text,
    Html,
    Csv,
    /// Everything else. It can be saved but never opened from the app: the system would pick a program for it by its extension,
    /// and an extension is the one thing the sender chose.
    Other,
}

impl MediaType {
    pub const ALL: [MediaType; 9] =
        [MediaType::Jpeg, MediaType::Png, MediaType::Gif, MediaType::Webp, MediaType::Pdf, MediaType::Text, MediaType::Html, MediaType::Csv, MediaType::Other];

    pub fn as_str(self) -> &'static str {
        match self {
            MediaType::Jpeg => "image/jpeg",
            MediaType::Png => "image/png",
            MediaType::Gif => "image/gif",
            MediaType::Webp => "image/webp",
            MediaType::Pdf => "application/pdf",
            MediaType::Text => "text/plain",
            MediaType::Html => "text/html",
            MediaType::Csv => "text/csv",
            MediaType::Other => "application/octet-stream",
        }
    }

    pub fn parse(text: &str) -> Option<MediaType> {
        MediaType::ALL.into_iter().find(|t| t.as_str() == text)
    }

    pub fn is_image(self) -> bool {
        matches!(self, MediaType::Jpeg | MediaType::Png | MediaType::Gif | MediaType::Webp)
    }

    /// The extension of a copy opened by the system's viewer. It comes from the kind of file the app found, never from the name.
    /// `None` means the file is not opened from the app.
    pub fn open_extension(self) -> Option<&'static str> {
        match self {
            MediaType::Jpeg => Some("jpg"),
            MediaType::Png => Some("png"),
            MediaType::Gif => Some("gif"),
            MediaType::Webp => Some("webp"),
            MediaType::Pdf => Some("pdf"),
            MediaType::Text => Some("txt"),
            MediaType::Html => Some("html"),
            MediaType::Csv => Some("csv"),
            MediaType::Other => None,
        }
    }
}

// -- names ----------------------------------------------------------------------------------------------------------------

/// Characters that are invisible or that reorder text, on top of the controls: the ones a token's name already refuses (direction
/// marks and overrides, the byte order mark), plus zero-width characters, the soft hyphen and the Arabic letter mark. A name shown
/// to a person must not read one way and be another.
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00ad}' | '\u{061c}' | '\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206f}' | '\u{feff}'
        )
}

/// Removes what is hidden, turns every kind of white space into one plain space (and folds runs of them), and cuts a long name to
/// the limit, keeping a short extension. Path separators are not touched here.
fn sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(4 * NAME_MAX_CHARS));
    let mut space = false;
    for c in raw.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
        } else if !is_hidden(c) {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    cut(out)
}

fn cut(name: String) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= NAME_MAX_CHARS {
        return name;
    }
    let extension_start = chars
        .iter()
        .rposition(|c| *c == '.')
        .filter(|&i| i > 0 && chars.len() - i <= EXTENSION_KEEP_CHARS && chars[i + 1..].iter().all(|c| c.is_alphanumeric()));
    let (stem, extension) = match extension_start {
        Some(i) => (&chars[..i], &chars[i..]),
        None => (&chars[..], &chars[chars.len()..]),
    };
    let room = NAME_MAX_CHARS - extension.len();
    let stem: String = stem.iter().take(room).collect();
    format!("{}{}", stem.trim_end(), extension.iter().collect::<String>())
}

fn only_dots(name: &str) -> bool {
    name.chars().all(|c| c == '.')
}

fn has_separator(name: &str) -> bool {
    name.contains(['/', '\\'])
}

/// The name of a document made from the name of a file a person picked, a command sent, or another device. What comes after the
/// last `/` or `\` is the name (some systems give the whole path). Never fails: a name with nothing left is `document`.
pub fn clean_file_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let name = sanitize(last);
    if only_dots(&name) {
        "document".to_string()
    } else {
        name
    }
}

/// A name a person typed. A separator in it is a character like any other, so it becomes a hyphen instead of cutting the name
/// short. `None` when nothing is left, which the caller answers as invalid input.
pub fn clean_title(raw: &str) -> Option<String> {
    let replaced: String = raw.chars().map(|c| if matches!(c, '/' | '\\') { '-' } else { c }).collect();
    let name = sanitize(&replaced);
    (!only_dots(&name)).then_some(name)
}

/// Whether a name from another device or a backup is one the app could have made. Such a row is refused, not repaired, so the two
/// devices cannot end up with different names for one document.
pub fn is_clean_name(name: &str) -> bool {
    !name.is_empty() && !has_separator(name) && !only_dots(name) && sanitize(name) == name
}

/// A name that is safe as a file name on every system, for a copy saved on request. It keeps what it can: the characters Windows
/// forbids become `_`, trailing dots and spaces go, and a reserved Windows name (`CON`, `NUL`, `COM1`, ...) gets an underscore in
/// front. The result is never empty.
pub fn disk_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || is_hidden(c) { '_' } else { c })
        .collect();
    while out.ends_with(['.', ' ']) {
        out.pop();
    }
    let out = out.trim_start().to_string();
    if out.is_empty() {
        return "document".to_string();
    }
    let stem = out.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0');
    if reserved {
        format!("_{out}")
    } else {
        out
    }
}

/// The name of the copy that the system's viewer is given: the name without its own extension, and the extension of the kind of
/// file the app found. `None` for a file that is not opened from the app.
pub fn open_name(name: &str, media: MediaType) -> Option<String> {
    let extension = media.open_extension()?;
    let stem = match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= EXTENSION_KEEP_CHARS + 1 && name[i + 1..].chars().all(|c| c.is_alphanumeric()) => &name[..i],
        _ => name,
    };
    // `disk_name` leaves a name that has a stem; the extension is added after, so a stem like `CON` is already made safe.
    Some(format!("{}.{extension}", disk_name(stem)))
}

// -- kinds of file ----------------------------------------------------------------------------------------------------------

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// What a file is, from its first bytes. The name counts only to tell a CSV or an HTML file from other text. An image whose
/// header cannot be read is `Other`: it would not be drawn, and it must not be handed to a viewer under an image's name.
pub fn sniff(name: &str, bytes: &[u8]) -> MediaType {
    let image = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(MediaType::Jpeg)
    } else if bytes.starts_with(&PNG_SIGNATURE) {
        Some(MediaType::Png)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(MediaType::Gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(MediaType::Webp)
    } else {
        None
    };
    if let Some(kind) = image {
        return if image_size(bytes, kind).is_some() { kind } else { MediaType::Other };
    }
    if bytes.starts_with(b"%PDF-") {
        return MediaType::Pdf;
    }
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return MediaType::Other;
    }
    let lower = name.to_lowercase();
    if lower.ends_with(".csv") {
        MediaType::Csv
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        MediaType::Html
    } else {
        MediaType::Text
    }
}

fn nonzero(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

fn be16(b: &[u8], at: usize) -> u32 {
    u32::from(u16::from_be_bytes([b[at], b[at + 1]]))
}

fn le16(b: &[u8], at: usize) -> u32 {
    u32::from(u16::from_le_bytes([b[at], b[at + 1]]))
}

fn le24(b: &[u8], at: usize) -> u32 {
    u32::from(b[at]) | u32::from(b[at + 1]) << 8 | u32::from(b[at + 2]) << 16
}

/// The width and height an image says it has, read from its header alone. `None` if the header is not one of a PNG, JPEG, GIF or
/// WebP, or is cut short. Nothing is decoded, so a hostile file can do no more than lie about its size, and that is what
/// `drawable` checks.
pub fn image_size(bytes: &[u8], kind: MediaType) -> Option<(u32, u32)> {
    match kind {
        MediaType::Png => {
            if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
                return None;
            }
            nonzero(u32::from_be_bytes(bytes[16..20].try_into().ok()?), u32::from_be_bytes(bytes[20..24].try_into().ok()?))
        }
        MediaType::Gif => {
            if bytes.len() < 10 {
                return None;
            }
            nonzero(le16(bytes, 6), le16(bytes, 8))
        }
        MediaType::Jpeg => jpeg_size(bytes),
        MediaType::Webp => webp_size(bytes),
        _ => None,
    }
}

fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    // a JPEG that needs more segments than this before its frame header is not one the app will draw
    for _ in 0..4096 {
        if *b.get(i)? != 0xff {
            return None;
        }
        while *b.get(i)? == 0xff {
            i += 1;
        }
        let marker = b[i];
        i += 1;
        match marker {
            // markers without a length: padding, the temporary one, and the restart markers
            0x00 | 0x01 | 0xd0..=0xd7 => continue,
            // the end, or the start of the image data, before any frame header
            0xd8..=0xda => return None,
            _ => {}
        }
        if i + 2 > b.len() {
            return None;
        }
        let length = be16(b, i) as usize;
        if length < 2 {
            return None;
        }
        let start_of_frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if start_of_frame {
            // length (2), precision (1), height (2), width (2)
            if i + 7 > b.len() {
                return None;
            }
            return nonzero(be16(b, i + 5), be16(b, i + 3));
        }
        i += length;
    }
    None
}

fn webp_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 25 || &b[..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    match &b[12..16] {
        // lossy: a frame tag of 3 bytes, the start code 9d 01 2a, then two 14-bit sizes
        b"VP8 " if b.len() >= 30 && b[23..26] == [0x9d, 0x01, 0x2a] => nonzero(le16(b, 26) & 0x3fff, le16(b, 28) & 0x3fff),
        // lossless: the signature 0x2f, then two 14-bit sizes, each less one
        b"VP8L" if b[20] == 0x2f => {
            let bits = u32::from_le_bytes(b[21..25].try_into().ok()?);
            nonzero((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        // extended: the canvas, as 24-bit sizes, each less one
        b"VP8X" if b.len() >= 30 => nonzero(le24(b, 24) + 1, le24(b, 27) + 1),
        _ => None,
    }
}

/// Whether the app may draw this image itself: it is an image, its header is readable and it is not larger than the limit.
pub fn drawable(bytes: &[u8], kind: MediaType) -> bool {
    kind.is_image() && image_size(bytes, kind).is_some_and(|(w, h)| u64::from(w) * u64::from(h) <= MAX_IMAGE_PIXELS)
}
