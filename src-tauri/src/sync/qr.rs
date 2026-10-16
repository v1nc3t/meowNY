//! What a QR code holds, and how it is read back (`docs/sync-design.md`, section 4).
//!
//! The text is `MNY1:` and then these bytes in base64url without padding, all numbers big-endian:
//! format version (1), kind, session id (8), secret (32), port (2), number of addresses (1 to 4), then for each address
//! a family byte (4 or 6) and 4 or 16 address bytes. With one IPv4 address that is 50 bytes.
//!
//! The text on a code is untrusted input, because a code can be made by anyone and shown to the phone. It is read
//! strictly (no padding, no spare bytes, no unused bits) and it may only name addresses on a home network: a code that
//! names a public address is refused, so a made-up code cannot make the phone connect to anything out on the internet.
//! This module draws nothing; the pictures are made by the screens.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use zeroize::Zeroizing;

use super::crypto::{Kind, Secret, SECRET_LEN, SESSION_ID_LEN};

pub const PREFIX: &str = "MNY1:";
/// The longest text that is read at all: the code with four IPv6 addresses is 156 characters.
pub const MAX_TEXT_LEN: usize = 200;
pub const MAX_ADDRESSES: usize = 4;
const FORMAT: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrError {
    /// Not text of this form.
    Text,
    /// A format from a newer version of the app.
    Version,
    Kind,
    /// The wrong number of bytes, or bytes left over.
    Length,
    Port,
    /// A family byte that is not 4 or 6, or no address, or too many.
    Address,
    /// An address that is not on a home network.
    NotLocal,
}

impl std::fmt::Display for QrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            QrError::Text => "this is not a meowNY code",
            QrError::Version => "this code is from a newer version of the app",
            QrError::Kind => "this code is not for this",
            QrError::Length => "this code is damaged",
            QrError::Port => "this code is damaged",
            QrError::Address => "this code is damaged",
            QrError::NotLocal => "this code points outside your home network",
        })
    }
}

impl std::error::Error for QrError {}

/// What a code says.
pub struct Payload {
    pub kind: Kind,
    pub session_id: [u8; SESSION_ID_LEN],
    /// The one-time secret of a sync, or the new pairing secret of a pairing.
    pub secret: Secret,
    pub port: u16,
    pub addresses: Vec<IpAddr>,
}

impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payload")
            .field("kind", &self.kind)
            .field("port", &self.port)
            .field("addresses", &self.addresses)
            .field("secret", &"(hidden)")
            .finish()
    }
}

/// An address on a home network: private, link-local or loopback for IPv4; unique-local or loopback for IPv6. (An IPv6
/// link-local address needs a scope id that a code cannot carry, so it is not accepted.)
pub fn is_local(address: &IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
    }
}

impl Payload {
    pub fn socket_addresses(&self) -> Vec<SocketAddr> {
        self.addresses.iter().map(|ip| SocketAddr::new(*ip, self.port)).collect()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, QrError> {
        if self.port == 0 {
            return Err(QrError::Port);
        }
        if self.addresses.is_empty() || self.addresses.len() > MAX_ADDRESSES {
            return Err(QrError::Address);
        }
        if !self.addresses.iter().all(is_local) {
            return Err(QrError::NotLocal);
        }
        let mut out = Vec::with_capacity(2 + SESSION_ID_LEN + SECRET_LEN + 3 + 17 * self.addresses.len());
        out.push(FORMAT);
        out.push(self.kind as u8);
        out.extend_from_slice(&self.session_id);
        out.extend_from_slice(self.secret.as_ref());
        out.extend_from_slice(&self.port.to_be_bytes());
        out.push(self.addresses.len() as u8);
        for address in &self.addresses {
            match address {
                IpAddr::V4(ip) => {
                    out.push(4);
                    out.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    out.push(6);
                    out.extend_from_slice(&ip.octets());
                }
            }
        }
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Payload, QrError> {
        let fixed = 2 + SESSION_ID_LEN + SECRET_LEN + 2 + 1;
        if bytes.len() < fixed {
            return Err(QrError::Length);
        }
        if bytes[0] > FORMAT {
            return Err(QrError::Version);
        }
        if bytes[0] != FORMAT {
            return Err(QrError::Text);
        }
        let kind = Kind::from_byte(bytes[1]).ok_or(QrError::Kind)?;
        let session_id: [u8; SESSION_ID_LEN] = bytes[2..2 + SESSION_ID_LEN].try_into().expect("8 bytes");
        let secret: [u8; SECRET_LEN] = bytes[2 + SESSION_ID_LEN..2 + SESSION_ID_LEN + SECRET_LEN].try_into().expect("32 bytes");
        let at = 2 + SESSION_ID_LEN + SECRET_LEN;
        let port = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        if port == 0 {
            return Err(QrError::Port);
        }
        let count = bytes[at + 2] as usize;
        if count == 0 || count > MAX_ADDRESSES {
            return Err(QrError::Address);
        }
        let mut rest = &bytes[fixed..];
        let mut addresses = Vec::with_capacity(count);
        for _ in 0..count {
            let (&family, tail) = rest.split_first().ok_or(QrError::Length)?;
            let size = match family {
                4 => 4,
                6 => 16,
                _ => return Err(QrError::Address),
            };
            if tail.len() < size {
                return Err(QrError::Length);
            }
            let (octets, tail) = tail.split_at(size);
            let address = if size == 4 {
                IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(octets).expect("4 bytes")))
            } else {
                IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(octets).expect("16 bytes")))
            };
            if !is_local(&address) {
                return Err(QrError::NotLocal);
            }
            addresses.push(address);
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(QrError::Length);
        }
        Ok(Payload { kind, session_id, secret: Zeroizing::new(secret), port, addresses })
    }

    pub fn to_text(&self) -> Result<String, QrError> {
        Ok(format!("{PREFIX}{}", base64url_encode(&self.to_bytes()?)))
    }

    pub fn from_text(text: &str) -> Result<Payload, QrError> {
        if text.len() > MAX_TEXT_LEN || !text.is_ascii() {
            return Err(QrError::Text);
        }
        let body = text.strip_prefix(PREFIX).ok_or(QrError::Text)?;
        let bytes = Zeroizing::new(base64url_decode(body).ok_or(QrError::Text)?);
        Payload::from_bytes(&bytes)
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding (RFC 4648, section 5).
pub fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// The value of each byte of the alphabet, and 255 for every other byte. Made at compile time: a document is decoded from text of
/// megabytes, and looking each character up in the alphabet would be slow.
const VALUES: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

/// The strict reverse: only the alphabet, no padding, and no unused bits left set, so each byte string has one text.
pub fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut values = Vec::with_capacity(text.len());
    for byte in text.bytes() {
        match VALUES[byte as usize] {
            255 => return None,
            value => values.push(u32::from(value)),
        }
    }
    let mut out = Vec::with_capacity(values.len() * 3 / 4);
    for group in values.chunks(4) {
        let mut n = 0u32;
        for (i, v) in group.iter().enumerate() {
            n |= v << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        let used = group.len() * 6 / 8;
        out.extend_from_slice(&bytes[1..1 + used]);
        // the bits after the last whole byte must be zero
        let spare_bits = group.len() * 6 - used * 8;
        if spare_bits > 0 && (n >> (24 - group.len() * 6)) & ((1 << spare_bits) - 1) != 0 {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys;

    // From `python3 scripts/sync-vectors.py --rust`.
    const QR_SYNC: &str = "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ";
    const QR_PAIR: &str = "MNY1:AQIRIjNEVWZ3iAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gw1ECBAoAAAUG_QAAAAAAAAAAAAAAAAAAAQ";

    fn sync_payload() -> Payload {
        Payload {
            kind: Kind::Sync,
            session_id: keys::hex_decode::<8>("1122334455667788").unwrap(),
            secret: Zeroizing::new(keys::hex_decode::<32>("4142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f60").unwrap()),
            port: 49152,
            addresses: vec!["192.168.1.20".parse().unwrap()],
        }
    }

    #[test]
    fn the_text_is_the_one_the_reference_implementation_makes() {
        assert_eq!(sync_payload().to_text().unwrap(), QR_SYNC);
        let pair = Payload {
            kind: Kind::Pair,
            secret: Zeroizing::new(keys::hex_decode::<32>("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20").unwrap()),
            port: 50001,
            addresses: vec!["10.0.0.5".parse().unwrap(), "fd00::1".parse().unwrap()],
            ..sync_payload()
        };
        assert_eq!(pair.to_text().unwrap(), QR_PAIR);
    }

    #[test]
    fn a_code_reads_back_as_it_was_made() {
        for text in [QR_SYNC, QR_PAIR] {
            let payload = Payload::from_text(text).unwrap();
            assert_eq!(payload.to_text().unwrap(), text);
        }
        let p = Payload::from_text(QR_PAIR).unwrap();
        assert_eq!((p.kind, p.port, p.addresses.len()), (Kind::Pair, 50001, 2));
        assert_eq!(p.socket_addresses()[1], "[fd00::1]:50001".parse().unwrap());
        assert_eq!(QR_SYNC.len(), 5 + 67, "a sync code with one IPv4 address is short enough to scan easily");
    }

    #[test]
    fn the_longest_code_fits_the_limit() {
        let p = Payload {
            addresses: vec!["fd00::1".parse().unwrap(), "fd00::2".parse().unwrap(), "fd00::3".parse().unwrap(), "fd00::4".parse().unwrap()],
            ..sync_payload()
        };
        let text = p.to_text().unwrap();
        assert!(text.len() <= MAX_TEXT_LEN, "{}", text.len());
        assert_eq!(Payload::from_text(&text).unwrap().addresses.len(), 4);
    }

    #[test]
    fn only_addresses_on_a_home_network_are_made_or_read() {
        for good in ["192.168.0.1", "10.1.2.3", "172.16.0.9", "172.31.255.255", "169.254.1.1", "127.0.0.1", "fd12:3456::1", "fc00::1", "::1"] {
            assert!(is_local(&good.parse().unwrap()), "{good}");
        }
        for bad in [
            "8.8.8.8", "1.1.1.1", "172.15.0.1", "172.32.0.1", "100.64.0.1", "0.0.0.0", "255.255.255.255", "224.0.0.1", "192.0.2.1",
            "::", "fe80::1", "2001:db8::1", "2606:4700::1111", "ff02::1", "::ffff:192.168.0.1",
        ] {
            assert!(!is_local(&bad.parse().unwrap()), "{bad}");
            let p = Payload { addresses: vec![bad.parse().unwrap()], ..sync_payload() };
            assert_eq!(p.to_text().err(), Some(QrError::NotLocal), "{bad} is not put in a code");
        }
    }

    #[test]
    fn a_code_that_names_a_public_address_is_refused_when_read() {
        let mut bytes = sync_payload().to_bytes().unwrap();
        let at = bytes.len() - 4;
        bytes[at..].copy_from_slice(&[8, 8, 8, 8]);
        assert_eq!(Payload::from_bytes(&bytes).err(), Some(QrError::NotLocal));
        // one public address among good ones is enough to refuse the whole code
        let mut mixed = sync_payload();
        mixed.addresses = vec!["192.168.1.2".parse().unwrap(), "10.0.0.1".parse().unwrap()];
        let mut bytes = mixed.to_bytes().unwrap();
        let at = bytes.len() - 4;
        bytes[at..].copy_from_slice(&[93, 184, 216, 34]);
        assert_eq!(Payload::from_bytes(&bytes).err(), Some(QrError::NotLocal));
    }

    #[test]
    fn damaged_or_foreign_codes_are_refused() {
        let good = sync_payload().to_bytes().unwrap();
        let with = |f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            f(&mut b);
            Payload::from_bytes(&b).err()
        };
        assert_eq!(with(&|b| b[0] = 2), Some(QrError::Version));
        assert_eq!(with(&|b| b[0] = 0), Some(QrError::Text));
        assert_eq!(with(&|b| b[1] = 3), Some(QrError::Kind));
        assert_eq!(with(&|b| b[1] = 0), Some(QrError::Kind));
        assert_eq!(with(&|b| { b[42] = 0; b[43] = 0 }), Some(QrError::Port));
        assert_eq!(with(&|b| b[44] = 0), Some(QrError::Address), "no address");
        assert_eq!(with(&|b| b[44] = 5), Some(QrError::Address), "more than four");
        assert_eq!(with(&|b| b[45] = 5), Some(QrError::Address), "a family that is not 4 or 6");
        assert_eq!(with(&|b| b[45] = 6), Some(QrError::Length), "a family that does not match the bytes left");
        assert_eq!(with(&|b| b.push(0)), Some(QrError::Length), "bytes left over");
        assert_eq!(with(&|b| { b.pop(); }), Some(QrError::Length));
        assert_eq!(with(&|b| b.truncate(10)), Some(QrError::Length));
        assert_eq!(with(&|b| b.clear()), Some(QrError::Length));
    }

    #[test]
    fn every_shortened_text_is_refused() {
        for end in 0..QR_SYNC.len() {
            assert!(Payload::from_text(&QR_SYNC[..end]).is_err(), "{end}");
        }
        for end in 0..QR_PAIR.len() {
            assert!(Payload::from_text(&QR_PAIR[..end]).is_err(), "{end}");
        }
    }

    #[test]
    fn text_that_is_not_a_code_is_refused() {
        for bad in [
            "",
            "MNY1:",
            "mny1:AQER",
            "MNY2:AQERIjNEVWZ3iEFC",
            "http://192.168.1.1/",
            " MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ",
            "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ=",
            "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ\n",
            "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoAR+",
            "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoAR/",
            "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQé",
        ] {
            assert!(Payload::from_text(bad).is_err(), "{bad:?}");
        }
        let long = format!("{PREFIX}{}", "A".repeat(MAX_TEXT_LEN));
        assert_eq!(Payload::from_text(&long).err(), Some(QrError::Text));
    }

    #[test]
    fn base64url_is_strict_and_round_trips() {
        for length in 0..40usize {
            let data: Vec<u8> = (0..length).map(|i| (i * 37 + 11) as u8).collect();
            let text = base64url_encode(&data);
            assert!(!text.contains('=') && !text.contains('+') && !text.contains('/'));
            assert_eq!(base64url_decode(&text).unwrap(), data);
        }
        assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url_encode(&[0xfb, 0xff]), "-_8");
        // RFC 4648 test vectors, without padding
        for (plain, coded) in [("", ""), ("f", "Zg"), ("fo", "Zm8"), ("foo", "Zm9v"), ("foob", "Zm9vYg"), ("fooba", "Zm9vYmE"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64url_encode(plain.as_bytes()), coded);
            assert_eq!(base64url_decode(coded).unwrap(), plain.as_bytes());
        }
        // not the canonical text of those bytes (spare bits set), a bad length, padding, other alphabets, spaces
        for bad in ["Zh", "Zm9", "Z", "Zm9vY", "Zg==", "Zm9v YmFy", "Zm+v", "Zm/v", "Zm9v\n", "Zm9vYmF\u{e9}"] {
            assert!(base64url_decode(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_secret_does_not_show_in_debug_output() {
        let text = format!("{:?}", sync_payload());
        assert!(text.contains("(hidden)") && !text.contains("65") && !text.contains("0x41"), "{text}");
    }

    #[test]
    fn random_text_is_refused_without_panicking() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_=+/ :\n\u{e9}\u{1F600}".chars().collect();
        for _ in 0..4000 {
            let length = (next() % 220) as usize;
            let body: String = (0..length).map(|_| chars[(next() as usize) % chars.len()]).collect();
            let _ = Payload::from_text(&body);
            let _ = Payload::from_text(&format!("{PREFIX}{body}"));
        }
        // bytes: random bodies behind a believable start never give a code with a public address
        for _ in 0..4000 {
            let mut bytes = vec![1u8, 1];
            bytes.extend((0..(next() % 80) as usize).map(|_| next() as u8));
            if let Ok(p) = Payload::from_bytes(&bytes) {
                assert!(p.addresses.iter().all(is_local));
                assert!(p.port != 0 && !p.addresses.is_empty() && p.addresses.len() <= MAX_ADDRESSES);
            }
        }
    }
}
