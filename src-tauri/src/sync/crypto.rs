//! The handshake and the sealed frames of a sync (`docs/sync-design.md`, sections 6 and 7).
//!
//! Made only of standard parts the app already uses: HKDF-SHA256 for keys and ChaCha20-Poly1305 for every message.
//! There is no Diffie-Hellman. Both sides hold a pairing secret from when they were paired, and a sync adds a one-time
//! secret that the desktop shows in a QR code, so a sync needs both the paired phone and the screen at that moment.
//! `scripts/sync-vectors.py` is a second implementation written from the RFCs; its output is in the tests below.
//!
//! No function here reads the network, the clock or the database. The caller supplies the bytes, and the errors say as
//! little as possible: a peer must not learn why a message was refused.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::keys;

pub const MAGIC: [u8; 4] = *b"MNY1";
pub const SECRET_LEN: usize = 32;
pub const SESSION_ID_LEN: usize = 8;
pub const NONCE_RANDOM_LEN: usize = 32;
pub const TAG_LEN: usize = 16;
/// The first message, phone to desktop: magic, kind, session id, a random value, and the sealed word "hello".
pub const M1_LEN: usize = 4 + 1 + SESSION_ID_LEN + NONCE_RANDOM_LEN + 5 + TAG_LEN;
/// The reply, desktop to phone: a random value and the sealed word "reply".
pub const M2_LEN: usize = NONCE_RANDOM_LEN + 5 + TAG_LEN;
/// The most one frame carries. Longer messages are split before they are sealed.
pub const MAX_PLAINTEXT: usize = 64 * 1024;
/// The most one session carries in all, in either direction.
pub const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;

const DIR_PHONE_TO_DESKTOP: u8 = 1;
const DIR_DESKTOP_TO_PHONE: u8 = 2;

/// A 32-byte secret: the pairing secret, or the one-time secret of one sync.
pub type Secret = Zeroizing<[u8; SECRET_LEN]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sync = 1,
    Pair = 2,
}

impl Kind {
    pub fn from_byte(byte: u8) -> Option<Kind> {
        match byte {
            1 => Some(Kind::Sync),
            2 => Some(Kind::Pair),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// A message of the wrong size.
    Length,
    /// Not a message of this protocol, or of another kind than the open window takes.
    Format,
    /// Made for another session.
    Session,
    /// It did not open: wrong secrets, changed on the way, repeated, out of order or sent back.
    Unauthenticated,
    /// A frame over the limit.
    TooLarge,
    /// A limit of the session was reached.
    Exhausted,
    /// The session was ended by an earlier failure.
    Closed,
    Random,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CryptoError::Length => "a message of the wrong size",
            CryptoError::Format => "not a message of this protocol",
            CryptoError::Session => "a message for another session",
            CryptoError::Unauthenticated => "a message that did not open",
            CryptoError::TooLarge => "a frame that is too large",
            CryptoError::Exhausted => "the session reached its limit",
            CryptoError::Closed => "the session is closed",
            CryptoError::Random => "no random numbers",
        })
    }
}

impl std::error::Error for CryptoError {}

impl From<keys::KeyError> for CryptoError {
    fn from(_: keys::KeyError) -> Self {
        CryptoError::Random
    }
}

/// The key material of a session: the pairing secret, and for a sync also the one-time secret. For a pairing the secret
/// in the QR code is the new pairing secret and there is nothing else.
#[derive(Clone)]
pub struct Ikm(Zeroizing<Vec<u8>>);

impl Ikm {
    pub fn for_sync(pairing: &[u8; SECRET_LEN], one_time: &[u8; SECRET_LEN]) -> Ikm {
        let mut bytes = Zeroizing::new(Vec::with_capacity(2 * SECRET_LEN));
        bytes.extend_from_slice(pairing);
        bytes.extend_from_slice(one_time);
        Ikm(bytes)
    }

    pub fn for_pairing(new_pairing: &[u8; SECRET_LEN]) -> Ikm {
        Ikm(Zeroizing::new(new_pairing.to_vec()))
    }
}

fn extract(salt: &[u8], ikm: &Ikm) -> Hkdf<Sha256> {
    Hkdf::<Sha256>::new(Some(salt), &ikm.0)
}

fn expand<const N: usize>(prk: &Hkdf<Sha256>, info: &[u8]) -> Zeroizing<[u8; N]> {
    let mut out = Zeroizing::new([0u8; N]);
    prk.expand(info, out.as_mut()).expect("an output of at most 255 hashes");
    out
}

fn salt(kind: Kind, session_id: &[u8; SESSION_ID_LEN], np: &[u8; NONCE_RANDOM_LEN], nd: Option<&[u8; NONCE_RANDOM_LEN]>) -> Vec<u8> {
    let mut salt = Vec::with_capacity(4 + 1 + SESSION_ID_LEN + 2 * NONCE_RANDOM_LEN);
    salt.extend_from_slice(&MAGIC);
    salt.push(kind as u8);
    salt.extend_from_slice(session_id);
    salt.extend_from_slice(np);
    if let Some(nd) = nd {
        salt.extend_from_slice(nd);
    }
    salt
}

/// ChaCha20-Poly1305 with the nonce of one of the two handshake messages. Each of these keys is used once, because the
/// random values that go into it are fresh, so the fixed nonce is never repeated under one key.
fn seal_once(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    ChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: plaintext, aad })
        .expect("sealing a short message cannot fail")
}

fn open_once(key: &[u8; 32], aad: &[u8], sealed: &[u8], expected: &[u8]) -> bool {
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: sealed, aad })
        .map(|plain| plain == expected)
        .unwrap_or(false)
}

fn frame_nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

fn frame_aad(direction: u8, counter: u64) -> [u8; 9] {
    let mut aad = [0u8; 9];
    aad[0] = direction;
    aad[1..].copy_from_slice(&counter.to_be_bytes());
    aad
}

/// What the phone holds between sending M1 and reading M2.
pub struct Initiator {
    kind: Kind,
    session_id: [u8; SESSION_ID_LEN],
    np: [u8; NONCE_RANDOM_LEN],
    ikm: Ikm,
    m1: [u8; M1_LEN],
}

impl Initiator {
    /// The phone's first message. `session_id` and the secrets come from the QR code (and the phone's pairing).
    pub fn start(kind: Kind, session_id: [u8; SESSION_ID_LEN], ikm: Ikm) -> Result<(Initiator, [u8; M1_LEN]), CryptoError> {
        Initiator::start_with(kind, session_id, ikm, keys::random_bytes::<NONCE_RANDOM_LEN>()?)
    }

    /// As `start` with the phone's random value given, for the test vectors.
    pub fn start_with(
        kind: Kind,
        session_id: [u8; SESSION_ID_LEN],
        ikm: Ikm,
        np: [u8; NONCE_RANDOM_LEN],
    ) -> Result<(Initiator, [u8; M1_LEN]), CryptoError> {
        let mut header = Vec::with_capacity(4 + 1 + SESSION_ID_LEN + NONCE_RANDOM_LEN);
        header.extend_from_slice(&MAGIC);
        header.push(kind as u8);
        header.extend_from_slice(&session_id);
        header.extend_from_slice(&np);
        let k1 = expand::<32>(&extract(&salt(kind, &session_id, &np, None), &ikm), b"k1");
        let mut m1 = [0u8; M1_LEN];
        m1[..header.len()].copy_from_slice(&header);
        m1[header.len()..].copy_from_slice(&seal_once(&k1, &header, b"hello"));
        Ok((Initiator { kind, session_id, np, ikm, m1 }, m1))
    }

    /// Reads the desktop's reply. If it opens, the desktop knows the secrets too, and the session is ready.
    pub fn finish(self, m2: &[u8]) -> Result<Session, CryptoError> {
        if m2.len() != M2_LEN {
            return Err(CryptoError::Length);
        }
        let nd: [u8; NONCE_RANDOM_LEN] = m2[..NONCE_RANDOM_LEN].try_into().expect("32 bytes");
        let prk = extract(&salt(self.kind, &self.session_id, &self.np, Some(&nd)), &self.ikm);
        let k2 = expand::<32>(&prk, b"k2");
        let mut aad = self.m1.to_vec();
        aad.extend_from_slice(&nd);
        if !open_once(&k2, &aad, &m2[NONCE_RANDOM_LEN..], b"reply") {
            return Err(CryptoError::Unauthenticated);
        }
        Ok(Session::new(&prk, DIR_PHONE_TO_DESKTOP))
    }
}

/// What the desktop holds after it has opened M1 and before it replies.
pub struct Accepted {
    kind: Kind,
    session_id: [u8; SESSION_ID_LEN],
    np: [u8; NONCE_RANDOM_LEN],
    ikm: Ikm,
    m1: [u8; M1_LEN],
    /// Which of the candidate secrets opened the message.
    pub index: usize,
}

/// The desktop reads M1. `kind` and `session_id` are those of the window that is open. `candidates` are the key materials
/// to try: in a sync window one per paired device, in a pairing window the new pairing secret. If none opens it, the
/// answer is an error and the caller closes the connection without sending anything.
pub fn accept(kind: Kind, session_id: &[u8; SESSION_ID_LEN], candidates: &[Ikm], m1: &[u8]) -> Result<Accepted, CryptoError> {
    if m1.len() != M1_LEN {
        return Err(CryptoError::Length);
    }
    if m1[..4] != MAGIC || Kind::from_byte(m1[4]) != Some(kind) {
        return Err(CryptoError::Format);
    }
    if m1[5..5 + SESSION_ID_LEN] != session_id[..] {
        return Err(CryptoError::Session);
    }
    let header_len = 4 + 1 + SESSION_ID_LEN + NONCE_RANDOM_LEN;
    let np: [u8; NONCE_RANDOM_LEN] = m1[5 + SESSION_ID_LEN..header_len].try_into().expect("32 bytes");
    // Every candidate is tried, so the time taken does not say which one it was.
    let mut found = None;
    for (index, ikm) in candidates.iter().enumerate() {
        let k1 = expand::<32>(&extract(&salt(kind, session_id, &np, None), ikm), b"k1");
        if open_once(&k1, &m1[..header_len], &m1[header_len..], b"hello") && found.is_none() {
            found = Some(index);
        }
    }
    let index = found.ok_or(CryptoError::Unauthenticated)?;
    Ok(Accepted {
        kind,
        session_id: *session_id,
        np,
        ikm: candidates[index].clone(),
        m1: m1.try_into().expect("checked length"),
        index,
    })
}

impl Accepted {
    pub fn reply(self) -> Result<([u8; M2_LEN], Session), CryptoError> {
        let nd = keys::random_bytes::<NONCE_RANDOM_LEN>()?;
        self.reply_with(nd)
    }

    /// As `reply` with the desktop's random value given, for the test vectors.
    pub fn reply_with(self, nd: [u8; NONCE_RANDOM_LEN]) -> Result<([u8; M2_LEN], Session), CryptoError> {
        let prk = extract(&salt(self.kind, &self.session_id, &self.np, Some(&nd)), &self.ikm);
        let k2 = expand::<32>(&prk, b"k2");
        let mut aad = self.m1.to_vec();
        aad.extend_from_slice(&nd);
        let mut m2 = [0u8; M2_LEN];
        m2[..NONCE_RANDOM_LEN].copy_from_slice(&nd);
        m2[NONCE_RANDOM_LEN..].copy_from_slice(&seal_once(&k2, &aad, b"reply"));
        Ok((m2, Session::new(&prk, DIR_DESKTOP_TO_PHONE)))
    }
}

/// An open session: one key and one counter for each direction.
pub struct Session {
    send_key: Secret,
    receive_key: Secret,
    send_direction: u8,
    receive_direction: u8,
    sent: u64,
    received: u64,
    bytes: u64,
    check: String,
    closed: bool,
}

impl Session {
    fn new(prk: &Hkdf<Sha256>, own_direction: u8) -> Session {
        let p2d = expand::<32>(prk, b"p2d");
        let d2p = expand::<32>(prk, b"d2p");
        let code = expand::<4>(prk, b"check");
        let (send_key, receive_key, other) = if own_direction == DIR_PHONE_TO_DESKTOP {
            (p2d, d2p, DIR_DESKTOP_TO_PHONE)
        } else {
            (d2p, p2d, DIR_PHONE_TO_DESKTOP)
        };
        Session {
            send_key,
            receive_key,
            send_direction: own_direction,
            receive_direction: other,
            sent: 0,
            received: 0,
            bytes: 0,
            check: format!("{:06}", u32::from_be_bytes(*code) % 1_000_000),
            closed: false,
        }
    }

    /// The six digits both screens show when a phone is paired.
    pub fn check_code(&self) -> &str {
        &self.check
    }

    fn count(&mut self, length: usize) -> Result<(), CryptoError> {
        self.bytes = self.bytes.saturating_add(length as u64);
        if self.bytes > MAX_SESSION_BYTES {
            self.closed = true;
            return Err(CryptoError::Exhausted);
        }
        Ok(())
    }

    /// Seals one frame. The result is what goes after the 4-byte length on the wire.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if self.closed {
            return Err(CryptoError::Closed);
        }
        if plaintext.len() > MAX_PLAINTEXT {
            return Err(CryptoError::TooLarge);
        }
        if self.sent == u64::MAX {
            self.closed = true;
            return Err(CryptoError::Exhausted);
        }
        self.count(plaintext.len())?;
        let sealed = ChaCha20Poly1305::new(Key::from_slice(self.send_key.as_ref()))
            .encrypt(
                Nonce::from_slice(&frame_nonce(self.sent)),
                Payload { msg: plaintext, aad: &frame_aad(self.send_direction, self.sent) },
            )
            .map_err(|_| CryptoError::Unauthenticated)?;
        self.sent += 1;
        Ok(sealed)
    }

    /// Opens the next frame. Any failure ends the session for good: nothing is read after a frame that did not open.
    pub fn open(&mut self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        if self.closed {
            return Err(CryptoError::Closed);
        }
        if sealed.len() < TAG_LEN {
            self.closed = true;
            return Err(CryptoError::Length);
        }
        if sealed.len() > MAX_PLAINTEXT + TAG_LEN {
            self.closed = true;
            return Err(CryptoError::TooLarge);
        }
        if self.received == u64::MAX {
            self.closed = true;
            return Err(CryptoError::Exhausted);
        }
        let plain = ChaCha20Poly1305::new(Key::from_slice(self.receive_key.as_ref()))
            .decrypt(
                Nonce::from_slice(&frame_nonce(self.received)),
                Payload { msg: sealed, aad: &frame_aad(self.receive_direction, self.received) },
            )
            .map(Zeroizing::new);
        let plain = match plain {
            Ok(plain) => plain,
            Err(_) => {
                self.closed = true;
                return Err(CryptoError::Unauthenticated);
            }
        };
        self.count(plain.len())?;
        self.received += 1;
        Ok(plain)
    }

    /// Ends the session; every later call fails.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

/// The 4 bytes that go in front of a sealed frame: its length.
pub fn frame_header(sealed_len: usize) -> [u8; 4] {
    (sealed_len as u32).to_be_bytes()
}

/// The length a frame header announces, if it is one this protocol allows. A peer cannot make the reader allocate more.
pub fn parse_frame_header(header: [u8; 4]) -> Result<usize, CryptoError> {
    let length = u32::from_be_bytes(header) as usize;
    if length < TAG_LEN {
        return Err(CryptoError::Length);
    }
    if length > MAX_PLAINTEXT + TAG_LEN {
        return Err(CryptoError::TooLarge);
    }
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Written by `python3 scripts/sync-vectors.py --rust`, a second implementation made from RFC 5869 and RFC 8439 that
    // checks itself against the RFCs' own test vectors before it prints anything.
    const PS: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const OTS: &str = "4142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f60";
    const SESSION_ID: &str = "1122334455667788";
    const NP: &str = "a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf";
    const ND: &str = "b0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecf";
    const SYNC_M1: &str = "4d4e5931011122334455667788a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebff9c52b1e5085d23c7659ae30c405807335af34e22b";
    const SYNC_M2: &str = "b0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecf6f898f2479c8af303bc106ed8861b54cb6e0f95bae";
    const SYNC_CHECK: &str = "038778";
    const PAIR_M1: &str = "4d4e5931021122334455667788a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc7cc40c8e974452f1134c97c65f918b71c961e2c9d";
    const PAIR_M2: &str = "b0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfc03d270e9def42c634738882c5119aa20746915c4b";
    const PAIR_CHECK: &str = "656199";
    const FRAME_P2D_0: &str = "081fcd37fc23696dfbbd41ce3211692a51ef7547fa5b873ff5f2ad7e82";
    const FRAME_P2D_1: &str = "73eeddea23ff89b46785f64bfacb8ce1939f7656e8f7";
    const FRAME_D2P_0: &str = "4eaab9a76c0568ed83ac3cc94e647a0aa751649109d502618c6707";

    fn hex<const N: usize>(text: &str) -> [u8; N] {
        keys::hex_decode::<N>(text).expect("test vector")
    }

    fn hex_vec(text: &str) -> Vec<u8> {
        (0..text.len() / 2).map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()).collect()
    }

    fn to_hex(bytes: &[u8]) -> String {
        keys::hex_encode(bytes)
    }

    fn ikm(kind: Kind) -> Ikm {
        match kind {
            Kind::Sync => Ikm::for_sync(&hex(PS), &hex(OTS)),
            Kind::Pair => Ikm::for_pairing(&hex(PS)),
        }
    }

    fn handshake(kind: Kind) -> (Session, Session, [u8; M1_LEN], [u8; M2_LEN]) {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (initiator, m1) = Initiator::start_with(kind, sid, ikm(kind), hex(NP)).unwrap();
        let accepted = accept(kind, &sid, &[ikm(kind)], &m1).unwrap();
        let (m2, desktop) = accepted.reply_with(hex(ND)).unwrap();
        let phone = initiator.finish(&m2).unwrap();
        (phone, desktop, m1, m2)
    }

    #[test]
    fn the_sizes_are_the_ones_in_the_design() {
        assert_eq!((M1_LEN, M2_LEN), (66, 53));
    }

    #[test]
    fn a_sync_handshake_gives_the_bytes_of_the_reference_implementation() {
        let (_, _, m1, m2) = handshake(Kind::Sync);
        assert_eq!(to_hex(&m1), SYNC_M1);
        assert_eq!(to_hex(&m2), SYNC_M2);
    }

    #[test]
    fn a_pairing_handshake_gives_the_bytes_of_the_reference_implementation() {
        let (_, _, m1, m2) = handshake(Kind::Pair);
        assert_eq!(to_hex(&m1), PAIR_M1);
        assert_eq!(to_hex(&m2), PAIR_M2);
    }

    #[test]
    fn both_sides_show_the_check_code_of_the_reference_implementation() {
        for (kind, code) in [(Kind::Sync, SYNC_CHECK), (Kind::Pair, PAIR_CHECK)] {
            let (phone, desktop, _, _) = handshake(kind);
            assert_eq!((phone.check_code(), desktop.check_code()), (code, code));
        }
    }

    #[test]
    fn frames_have_the_bytes_of_the_reference_implementation() {
        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        let first = phone.seal(b"hello desktop").unwrap();
        let second = phone.seal(b"second").unwrap();
        let reply = desktop.seal(b"hello phone").unwrap();
        assert_eq!((to_hex(&first).as_str(), to_hex(&second).as_str(), to_hex(&reply).as_str()), (FRAME_P2D_0, FRAME_P2D_1, FRAME_D2P_0));
        assert_eq!(desktop.open(&hex_vec(FRAME_P2D_0)).unwrap().as_slice(), b"hello desktop");
        assert_eq!(desktop.open(&hex_vec(FRAME_P2D_1)).unwrap().as_slice(), b"second");
        assert_eq!(phone.open(&hex_vec(FRAME_D2P_0)).unwrap().as_slice(), b"hello phone");
    }

    #[test]
    fn a_handshake_with_random_values_works_and_two_of_them_differ() {
        let sid = [7u8; SESSION_ID_LEN];
        let run = || {
            let (initiator, m1) = Initiator::start(Kind::Sync, sid, ikm(Kind::Sync)).unwrap();
            let (m2, mut desktop) = accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &m1).unwrap().reply().unwrap();
            let mut phone = initiator.finish(&m2).unwrap();
            let sealed = phone.seal(b"x").unwrap();
            assert_eq!(desktop.open(&sealed).unwrap().as_slice(), b"x");
            (m1, m2, sealed)
        };
        let (a, b) = (run(), run());
        assert_ne!(a.0, b.0);
        assert_ne!(a.1, b.1);
        assert_ne!(a.2, b.2, "fresh random values give fresh keys");
    }

    #[test]
    fn the_desktop_finds_which_paired_device_it_is_talking_to() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let other = |byte: u8| Ikm::for_sync(&[byte; 32], &hex(OTS));
        let candidates = [other(1), other(2), ikm(Kind::Sync), other(3)];
        let (_, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        assert_eq!(accept(Kind::Sync, &sid, &candidates, &m1).unwrap().index, 2);
        assert!(accept(Kind::Sync, &sid, &[other(1), other(2)], &m1).is_err());
        assert!(accept(Kind::Sync, &sid, &[], &m1).is_err());
    }

    #[test]
    fn either_secret_alone_is_not_enough() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (_, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        let only_pairing = Ikm::for_pairing(&hex(PS));
        let wrong_one_time = Ikm::for_sync(&hex(PS), &[0u8; 32]);
        let wrong_pairing = Ikm::for_sync(&[0u8; 32], &hex(OTS));
        for wrong in [only_pairing, wrong_one_time, wrong_pairing] {
            assert_eq!(accept(Kind::Sync, &sid, &[wrong], &m1).err(), Some(CryptoError::Unauthenticated));
        }
    }

    #[test]
    fn a_pairing_message_does_not_open_in_a_sync_window_and_the_other_way_round() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (_, pair_m1) = Initiator::start_with(Kind::Pair, sid, ikm(Kind::Pair), hex(NP)).unwrap();
        let (_, sync_m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        assert_eq!(accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &pair_m1).err(), Some(CryptoError::Format));
        assert_eq!(accept(Kind::Pair, &sid, &[ikm(Kind::Pair)], &sync_m1).err(), Some(CryptoError::Format));
    }

    #[test]
    fn a_message_for_another_session_is_refused() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (_, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        assert_eq!(accept(Kind::Sync, &[9u8; 8], &[ikm(Kind::Sync)], &m1).err(), Some(CryptoError::Session));
    }

    #[test]
    fn a_session_id_changed_in_flight_is_refused_even_when_the_window_is_told_so() {
        // an attacker who changes the id in M1 to the window's id still cannot make it open: the id is part of the sealed header
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (_, mut m1) = Initiator::start_with(Kind::Sync, [1u8; 8], ikm(Kind::Sync), hex(NP)).unwrap();
        m1[5..13].copy_from_slice(&sid);
        assert_eq!(accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &m1).err(), Some(CryptoError::Unauthenticated));
    }

    #[test]
    fn every_changed_bit_of_m1_is_refused() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (_, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        for byte in 0..M1_LEN {
            for bit in 0..8 {
                let mut bad = m1;
                bad[byte] ^= 1 << bit;
                assert!(accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &bad).is_err(), "byte {byte} bit {bit}");
            }
        }
    }

    #[test]
    fn every_changed_bit_of_m2_is_refused_by_the_phone() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        for byte in 0..M2_LEN {
            for bit in 0..8 {
                let (initiator, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
                let (mut m2, _) = accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &m1).unwrap().reply_with(hex(ND)).unwrap();
                m2[byte] ^= 1 << bit;
                assert!(initiator.finish(&m2).is_err(), "byte {byte} bit {bit}");
            }
        }
    }

    #[test]
    fn messages_of_the_wrong_size_are_refused() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (initiator, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        for bad in [&m1[..M1_LEN - 1], &[m1.as_slice(), &[0]].concat()[..], &[][..]] {
            assert_eq!(accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], bad).err(), Some(CryptoError::Length));
        }
        let (m2, _) = accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &m1).unwrap().reply_with(hex(ND)).unwrap();
        assert_eq!(initiator.finish(&m2[..M2_LEN - 1]).err(), Some(CryptoError::Length));
    }

    #[test]
    fn a_reply_from_a_desktop_that_does_not_know_the_secrets_is_refused() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (initiator, m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        // a fake desktop that saw M1 but has other secrets cannot even open it; one that answers anyway gets a bad reply
        let fake = Ikm::for_sync(&[5u8; 32], &[6u8; 32]);
        let (_, other_m1) = Initiator::start_with(Kind::Sync, sid, fake.clone(), hex(NP)).unwrap();
        let (m2, _) = accept(Kind::Sync, &sid, &[fake], &other_m1).unwrap().reply_with(hex(ND)).unwrap();
        assert_eq!(initiator.finish(&m2).err(), Some(CryptoError::Unauthenticated));
        let _ = m1;
    }

    #[test]
    fn a_reply_to_another_message_is_refused() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let (first, _) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
        let (_, second_m1) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), [0x55; 32]).unwrap();
        let (m2, _) = accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &second_m1).unwrap().reply_with(hex(ND)).unwrap();
        assert_eq!(first.finish(&m2).err(), Some(CryptoError::Unauthenticated), "M2 is bound to the M1 it answers");
    }

    #[test]
    fn frames_must_arrive_once_and_in_order() {
        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        let a = phone.seal(b"a").unwrap();
        let b = phone.seal(b"b").unwrap();
        // out of order
        assert_eq!(desktop.open(&b).err(), Some(CryptoError::Unauthenticated));
        assert_eq!(desktop.open(&a).err(), Some(CryptoError::Closed), "after a failure nothing more is read");

        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        let a = phone.seal(b"a").unwrap();
        desktop.open(&a).unwrap();
        // repeated
        assert!(desktop.open(&a).is_err());

        // skipped
        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        phone.seal(b"lost").unwrap();
        let second = phone.seal(b"b").unwrap();
        assert!(desktop.open(&second).is_err());
    }

    #[test]
    fn a_frame_sent_back_to_its_sender_does_not_open() {
        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        let from_phone = phone.seal(b"x").unwrap();
        assert!(phone.open(&from_phone).is_err(), "reflected");
        let from_desktop = desktop.seal(b"x").unwrap();
        let (_, mut desktop2, _, _) = handshake(Kind::Sync);
        assert!(desktop2.open(&from_desktop).is_err(), "a frame from the desktop does not open on the desktop");
    }

    #[test]
    fn every_changed_bit_of_a_frame_is_refused() {
        let (mut phone, _, _, _) = handshake(Kind::Sync);
        let frame = phone.seal(b"sixteen bytes!!!!").unwrap();
        for byte in 0..frame.len() {
            for bit in 0..8 {
                let (_, mut desktop, _, _) = handshake(Kind::Sync);
                let mut bad = frame.clone();
                bad[byte] ^= 1 << bit;
                assert!(desktop.open(&bad).is_err(), "byte {byte} bit {bit}");
            }
        }
    }

    #[test]
    fn frames_of_every_size_up_to_the_limit_round_trip_and_bigger_ones_are_refused() {
        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        for size in [0usize, 1, 15, 16, 17, 255, 4096, MAX_PLAINTEXT] {
            let data = vec![0xAB; size];
            let sealed = phone.seal(&data).unwrap();
            assert_eq!(sealed.len(), size + TAG_LEN);
            assert_eq!(desktop.open(&sealed).unwrap().as_slice(), data.as_slice());
        }
        assert_eq!(phone.seal(&vec![0; MAX_PLAINTEXT + 1]).err(), Some(CryptoError::TooLarge));
        assert_eq!(desktop.open(&vec![0; MAX_PLAINTEXT + TAG_LEN + 1]).err(), Some(CryptoError::TooLarge));
    }

    #[test]
    fn a_frame_shorter_than_its_tag_is_refused() {
        let (_, mut desktop, _, _) = handshake(Kind::Sync);
        assert_eq!(desktop.open(&[0u8; TAG_LEN - 1]).err(), Some(CryptoError::Length));
    }

    #[test]
    fn a_session_stops_at_its_byte_limit() {
        let (mut phone, mut desktop, _, _) = handshake(Kind::Sync);
        phone.bytes = MAX_SESSION_BYTES - 5;
        desktop.bytes = MAX_SESSION_BYTES - 5;
        let sealed = phone.seal(b"12345").unwrap();
        assert_eq!(desktop.open(&sealed).unwrap().as_slice(), b"12345");
        assert_eq!(phone.seal(b"x").err(), Some(CryptoError::Exhausted));
        assert_eq!(phone.seal(b"x").err(), Some(CryptoError::Closed));
    }

    #[test]
    fn a_closed_session_does_nothing_more() {
        let (mut phone, _, _, _) = handshake(Kind::Sync);
        phone.close();
        assert_eq!(phone.seal(b"x").err(), Some(CryptoError::Closed));
        assert_eq!(phone.open(&[0u8; 20]).err(), Some(CryptoError::Closed));
    }

    #[test]
    fn frame_headers_announce_only_sane_lengths() {
        assert_eq!(parse_frame_header(frame_header(16)).unwrap(), 16);
        assert_eq!(parse_frame_header(frame_header(MAX_PLAINTEXT + TAG_LEN)).unwrap(), MAX_PLAINTEXT + TAG_LEN);
        assert_eq!(parse_frame_header(frame_header(15)).err(), Some(CryptoError::Length));
        assert_eq!(parse_frame_header(frame_header(0)).err(), Some(CryptoError::Length));
        assert_eq!(parse_frame_header(frame_header(MAX_PLAINTEXT + TAG_LEN + 1)).err(), Some(CryptoError::TooLarge));
        assert_eq!(parse_frame_header([0xFF; 4]).err(), Some(CryptoError::TooLarge));
    }

    /// A small fixed-seed generator, as elsewhere in the project: no crate, the same run every time.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn bytes(&mut self, length: usize) -> Vec<u8> {
            (0..length).map(|_| self.next() as u8).collect()
        }
    }

    #[test]
    fn hostile_input_never_opens_and_never_panics() {
        let sid = hex::<SESSION_ID_LEN>(SESSION_ID);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for round in 0..3000 {
            let length = match round % 4 {
                0 => M1_LEN,
                1 => (rng.next() % 200) as usize,
                2 => M2_LEN,
                _ => (rng.next() % 70_000) as usize,
            };
            let mut bytes = rng.bytes(length);
            if round % 3 == 0 && bytes.len() >= 13 {
                // a believable start: the right magic, kind and session id, then noise
                bytes[..4].copy_from_slice(&MAGIC);
                bytes[4] = 1;
                bytes[5..13].copy_from_slice(&sid);
            }
            assert!(accept(Kind::Sync, &sid, &[ikm(Kind::Sync)], &bytes).is_err());
            assert!(accept(Kind::Pair, &sid, &[], &bytes).is_err());
            let (initiator, _) = Initiator::start_with(Kind::Sync, sid, ikm(Kind::Sync), hex(NP)).unwrap();
            assert!(initiator.finish(&bytes).is_err());
            let (_, mut desktop, _, _) = handshake(Kind::Sync);
            assert!(desktop.open(&bytes).is_err());
            let header: [u8; 4] = rng.bytes(4).try_into().unwrap();
            let _ = parse_frame_header(header);
        }
    }

    #[test]
    fn secrets_are_wiped_from_the_key_material_type() {
        // `Ikm` and the session keys are `Zeroizing`: this only checks that cloning and dropping them is allowed
        let a = ikm(Kind::Sync);
        let b = a.clone();
        drop(a);
        drop(b);
    }
}
