//! Key handling. The database key is a random 32-byte key that is never stored as-is. It is
//! kept only in wrapped (encrypted) copies, each opened by a different key-encryption key:
//!
//! * Everyday copy. Opened by HKDF(keychain secret [|| Argon2id(passphrase, salt)]). The random
//!   secret lives in the OS keychain, so with a passphrase the data needs both the keychain
//!   entry and the passphrase. Copying the file elsewhere gets nothing.
//! * Recovery copy. Opened by a random recovery key shown once at setup. It works without the
//!   keychain and without the passphrase, so it must be stored like a spare house key.
//!
//! Wrapping is ChaCha20-Poly1305, so a wrong key is detected before the database is touched.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

use crate::db::DbKey;

pub const SECRET_LEN: usize = 32;
pub const SALT_LEN: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub enum KeyError {
    /// The OS keychain is missing, locked or refused access.
    Store,
    /// The system could not provide randomness.
    Random,
    /// The stored secret is not what we wrote.
    Corrupt,
    /// The key did not open the wrapped copy: wrong passphrase or wrong recovery key.
    Wrong,
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::Store => write!(f, "the OS keychain is not available"),
            KeyError::Random => write!(f, "no secure randomness available"),
            KeyError::Corrupt => write!(f, "the stored key is damaged"),
            KeyError::Wrong => write!(f, "the key does not open this data"),
        }
    }
}

impl std::error::Error for KeyError {}

/// Where the random secret is kept. The real one is the OS keychain; tests use memory.
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<Zeroizing<[u8; SECRET_LEN]>>, KeyError>;
    fn set(&self, secret: &[u8; SECRET_LEN]) -> Result<(), KeyError>;
    fn delete(&self) -> Result<(), KeyError>;
}

#[cfg(desktop)]
pub struct KeychainStore {
    entry: keyring::Entry,
}

#[cfg(desktop)]
impl KeychainStore {
    pub fn new() -> Result<Self, KeyError> {
        let entry = keyring::Entry::new("org.meowny.app", "database-key").map_err(|_| KeyError::Store)?;
        Ok(Self { entry })
    }
}

#[cfg(desktop)]
impl SecretStore for KeychainStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; SECRET_LEN]>>, KeyError> {
        match self.entry.get_password() {
            Ok(hex) => decode_secret(&hex).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(KeyError::Store),
        }
    }

    fn set(&self, secret: &[u8; SECRET_LEN]) -> Result<(), KeyError> {
        let hex = Zeroizing::new(hex_encode(secret));
        self.entry.set_password(&hex).map_err(|_| KeyError::Store)
    }

    fn delete(&self) -> Result<(), KeyError> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(KeyError::Store),
        }
    }
}

/// The secret in a file next to the data, for a device with no keychain to put it in (the phone app). The file holds no more
/// protection than the folder does: the passphrase is what keeps the data closed, so a vault that uses this store must have one.
pub struct FileStore {
    path: std::path::PathBuf,
}

impl FileStore {
    pub fn new(path: std::path::PathBuf) -> FileStore {
        FileStore { path }
    }
}

impl SecretStore for FileStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; SECRET_LEN]>>, KeyError> {
        match std::fs::read_to_string(&self.path) {
            Ok(hex) => decode_secret(hex.trim()).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(KeyError::Store),
        }
    }

    fn set(&self, secret: &[u8; SECRET_LEN]) -> Result<(), KeyError> {
        use std::io::Write;
        let hex = Zeroizing::new(hex_encode(secret));
        let tmp = self.path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let written = options.open(&tmp).and_then(|mut f| f.write_all(hex.as_bytes()).and_then(|_| f.sync_all()));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return Err(KeyError::Store);
        }
        std::fs::rename(&tmp, &self.path).map_err(|_| {
            let _ = std::fs::remove_file(&tmp);
            KeyError::Store
        })
    }

    fn delete(&self) -> Result<(), KeyError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(KeyError::Store),
        }
    }
}

pub fn random_bytes<const N: usize>() -> Result<[u8; N], KeyError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| KeyError::Random)?;
    Ok(bytes)
}

pub fn new_secret() -> Result<Zeroizing<[u8; SECRET_LEN]>, KeyError> {
    Ok(Zeroizing::new(random_bytes::<SECRET_LEN>()?))
}

/// Argon2id cost. Tests use a cheap setting so they stay fast.
#[cfg(not(test))]
fn argon_params() -> Params {
    Params::new(64 * 1024, 3, 1, Some(32)).expect("valid argon2 params")
}

#[cfg(test)]
fn argon_params() -> Params {
    Params::new(8, 1, 1, Some(32)).expect("valid argon2 params")
}

/// A key-encryption key: opens one wrapped copy of the database key.
pub type Kek = Zeroizing<[u8; 32]>;

/// A fresh random database key.
pub fn new_db_key() -> Result<DbKey, KeyError> {
    let mut bytes = random_bytes::<32>()?;
    let key = DbKey::from_bytes(bytes);
    bytes.zeroize();
    Ok(key)
}

fn hkdf_key(salt: Option<&[u8]>, ikm: &[u8], info: &[u8]) -> Result<Kek, KeyError> {
    let mut out = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(salt, ikm).expand(info, out.as_mut()).map_err(|_| KeyError::Corrupt)?;
    Ok(out)
}

/// The everyday key-encryption key, from the keychain secret and an optional passphrase.
pub fn derive_kek(secret: &[u8; SECRET_LEN], passphrase: Option<&str>, salt: &[u8; SALT_LEN]) -> Result<Kek, KeyError> {
    let Some(passphrase) = passphrase else {
        return hkdf_key(Some(salt), secret, b"meowny kek v1");
    };

    let mut stretched = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params())
        .hash_password_into(passphrase.as_bytes(), salt, stretched.as_mut())
        .map_err(|_| KeyError::Corrupt)?;

    let mut ikm = Zeroizing::new([0u8; SECRET_LEN + 32]);
    ikm[..SECRET_LEN].copy_from_slice(secret);
    ikm[SECRET_LEN..].copy_from_slice(stretched.as_ref());
    hkdf_key(Some(salt), ikm.as_ref(), b"meowny kek v1")
}

#[derive(Clone, Copy)]
pub enum Purpose {
    Everyday,
    Recovery,
}

impl Purpose {
    /// Bound into each wrapped copy so one cannot be passed off as the other.
    fn aad(self) -> &'static [u8] {
        match self {
            Purpose::Everyday => b"meowny wrap v1 everyday",
            Purpose::Recovery => b"meowny wrap v1 recovery",
        }
    }
}

const NONCE_LEN: usize = 12;

/// Encrypts the database key under `kek`. Output is hex of nonce followed by ciphertext.
pub fn wrap(kek: &Kek, key: &DbKey, purpose: Purpose) -> Result<String, KeyError> {
    let nonce_bytes = random_bytes::<NONCE_LEN>()?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek.as_ref()));
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), Payload { msg: key.as_bytes(), aad: purpose.aad() })
        .map_err(|_| KeyError::Corrupt)?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&sealed);
    Ok(hex_encode(&out))
}

/// Opens a wrapped copy. A wrong key returns `Wrong`; malformed text returns `Corrupt`.
pub fn unwrap(kek: &Kek, wrapped: &str, purpose: Purpose) -> Result<DbKey, KeyError> {
    const TOTAL: usize = NONCE_LEN + 32 + 16;
    let bytes = hex_decode::<TOTAL>(wrapped).ok_or(KeyError::Corrupt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek.as_ref()));
    let opened = cipher
        .decrypt(Nonce::from_slice(&bytes[..NONCE_LEN]), Payload { msg: &bytes[NONCE_LEN..], aad: purpose.aad() })
        .map_err(|_| KeyError::Wrong)?;
    let opened = Zeroizing::new(opened);
    let key: [u8; 32] = opened.as_slice().try_into().map_err(|_| KeyError::Corrupt)?;
    Ok(DbKey::from_bytes(key))
}

const RECOVERY_BYTES: usize = 20;
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 160 random bits written as 32 characters in groups of four, e.g. `7F3K-9QXM-...`.
/// Random and long enough that no stretching is needed.
#[derive(zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct RecoveryKey([u8; RECOVERY_BYTES]);

impl RecoveryKey {
    pub fn generate() -> Result<Self, KeyError> {
        Ok(Self(random_bytes::<RECOVERY_BYTES>()?))
    }

    pub fn to_display(&self) -> String {
        let mut chars = Vec::with_capacity(32);
        let (mut buffer, mut bits) = (0u32, 0u32);
        for byte in self.0 {
            buffer = (buffer << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                chars.push(ALPHABET[((buffer >> bits) & 31) as usize]);
            }
            buffer &= (1 << bits) - 1;
        }
        chars
            .chunks(4)
            .map(|group| String::from_utf8_lossy(group).into_owned())
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Accepts any case, spaces or dashes between groups, and the usual look-alike letters (O for 0, I or L for 1).
    pub fn parse(text: &str) -> Option<Self> {
        let mut bytes = [0u8; RECOVERY_BYTES];
        let (mut buffer, mut bits, mut count, mut chars) = (0u32, 0u32, 0usize, 0usize);
        for c in text.chars().filter(|c| !c.is_whitespace() && *c != '-') {
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            };
            let value = ALPHABET.iter().position(|&a| a as char == c)? as u32;
            chars += 1;
            buffer = (buffer << 5) | value;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                if count == RECOVERY_BYTES {
                    return None;
                }
                bytes[count] = ((buffer >> bits) & 0xff) as u8;
                count += 1;
                buffer &= (1 << bits) - 1;
            }
        }
        (chars == 32 && count == RECOVERY_BYTES).then_some(Self(bytes))
    }

    /// The key-encryption key for the recovery copy.
    pub fn kek(&self) -> Result<Kek, KeyError> {
        hkdf_key(None, &self.0, b"meowny recovery kek v1")
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn decode_secret(hex: &str) -> Result<Zeroizing<[u8; SECRET_LEN]>, KeyError> {
    hex_decode::<SECRET_LEN>(hex).map(Zeroizing::new).ok_or(KeyError::Corrupt)
}

#[cfg(test)]
pub mod memory {
    use super::*;
    use std::sync::Mutex;

    /// In-memory stand-in for the keychain.
    #[derive(Default)]
    pub struct MemoryStore(pub Mutex<Option<[u8; SECRET_LEN]>>);

    impl SecretStore for MemoryStore {
        fn get(&self) -> Result<Option<Zeroizing<[u8; SECRET_LEN]>>, KeyError> {
            Ok(self.0.lock().unwrap().map(Zeroizing::new))
        }
        fn set(&self, secret: &[u8; SECRET_LEN]) -> Result<(), KeyError> {
            *self.0.lock().unwrap() = Some(*secret);
            Ok(())
        }
        fn delete(&self) -> Result<(), KeyError> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: [u8; 32] = [7; 32];
    const SALT: [u8; 16] = [9; 16];

    fn kek(passphrase: Option<&str>) -> Kek {
        derive_kek(&SECRET, passphrase, &SALT).unwrap()
    }

    #[test]
    fn passphrase_changes_the_key_encryption_key() {
        let plain = kek(None);
        let a = kek(Some("correct horse"));
        assert_eq!(*a, *kek(Some("correct horse")));
        assert_ne!(*a, *plain);
        assert_ne!(*a, *kek(Some("wrong horse")));
    }

    #[test]
    fn kek_depends_on_secret_and_salt() {
        let base = kek(Some("pw"));
        assert_ne!(*base, *derive_kek(&[8; 32], Some("pw"), &SALT).unwrap());
        assert_ne!(*base, *derive_kek(&SECRET, Some("pw"), &[1; 16]).unwrap());
        assert_ne!(*kek(None), *derive_kek(&SECRET, None, &[1; 16]).unwrap());
    }

    #[test]
    fn wrapped_key_round_trips_and_only_the_right_kek_opens_it() {
        let key = new_db_key().unwrap();
        let wrapped = wrap(&kek(Some("pw")), &key, Purpose::Everyday).unwrap();
        assert_eq!(unwrap(&kek(Some("pw")), &wrapped, Purpose::Everyday).unwrap().as_bytes(), key.as_bytes());
        assert_eq!(unwrap(&kek(Some("other")), &wrapped, Purpose::Everyday).err(), Some(KeyError::Wrong));
        assert_eq!(unwrap(&kek(None), &wrapped, Purpose::Everyday).err(), Some(KeyError::Wrong));
    }

    #[test]
    fn a_wrapped_copy_cannot_be_used_for_another_purpose() {
        let key = new_db_key().unwrap();
        let wrapped = wrap(&kek(None), &key, Purpose::Everyday).unwrap();
        assert_eq!(unwrap(&kek(None), &wrapped, Purpose::Recovery).err(), Some(KeyError::Wrong));
    }

    #[test]
    fn tampered_or_malformed_wrapped_keys_are_refused() {
        let key = new_db_key().unwrap();
        let mut wrapped = wrap(&kek(None), &key, Purpose::Everyday).unwrap();
        assert_eq!(unwrap(&kek(None), "abc", Purpose::Everyday).err(), Some(KeyError::Corrupt));
        let last = wrapped.pop().unwrap();
        wrapped.push(if last == '0' { '1' } else { '0' });
        assert_eq!(unwrap(&kek(None), &wrapped, Purpose::Everyday).err(), Some(KeyError::Wrong));
    }

    #[test]
    fn wrapping_twice_gives_different_text() {
        let key = new_db_key().unwrap();
        assert_ne!(wrap(&kek(None), &key, Purpose::Everyday).unwrap(), wrap(&kek(None), &key, Purpose::Everyday).unwrap());
    }

    #[test]
    fn recovery_key_displays_as_eight_groups_and_parses_back() {
        let key = RecoveryKey::generate().unwrap();
        let shown = key.to_display();
        assert_eq!(shown.len(), 8 * 4 + 7);
        assert!(shown.split('-').all(|g| g.len() == 4));
        let parsed = RecoveryKey::parse(&shown).unwrap();
        assert_eq!(*parsed.kek().unwrap(), *key.kek().unwrap());
    }

    #[test]
    fn recovery_key_parsing_is_forgiving_but_exact() {
        let key = RecoveryKey::generate().unwrap();
        let shown = key.to_display();
        let kek = key.kek().unwrap();
        for variant in [shown.to_lowercase(), shown.replace('-', " "), shown.replace('-', ""), format!("  {shown}  ")] {
            assert_eq!(*RecoveryKey::parse(&variant).unwrap().kek().unwrap(), *kek, "{variant}");
        }
        assert!(RecoveryKey::parse("").is_none());
        assert!(RecoveryKey::parse(&shown[..shown.len() - 1]).is_none(), "too short");
        assert!(RecoveryKey::parse(&format!("{shown}A")).is_none(), "too long");
        assert!(RecoveryKey::parse("UUUU-UUUU-UUUU-UUUU-UUUU-UUUU-UUUU-UUUU").is_none(), "U is not in the alphabet");
    }

    #[test]
    fn look_alike_letters_are_read_as_digits() {
        let a = RecoveryKey::parse("0000-0000-0000-0000-0000-0000-0000-0001").unwrap();
        let b = RecoveryKey::parse("OOOO-oooo-0000-0000-0000-0000-0000-000L").unwrap();
        assert_eq!(*a.kek().unwrap(), *b.kek().unwrap());
    }

    #[test]
    fn recovery_keys_differ_each_time() {
        assert_ne!(RecoveryKey::generate().unwrap().to_display(), RecoveryKey::generate().unwrap().to_display());
    }

    #[test]
    fn a_secret_in_a_file_is_stored_private_read_back_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("phone.secret"));
        assert!(store.get().unwrap().is_none());
        store.set(&[0xAB; SECRET_LEN]).unwrap();
        assert_eq!(*store.get().unwrap().unwrap(), [0xAB; SECRET_LEN]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("phone.secret")).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "readable by the owner only");
        }
        assert!(!dir.path().join("phone.tmp").exists(), "no temporary file is left");
        store.set(&[0xCD; SECRET_LEN]).unwrap();
        assert_eq!(*store.get().unwrap().unwrap(), [0xCD; SECRET_LEN], "a new secret replaces the old");
        store.delete().unwrap();
        store.delete().unwrap();
        assert!(store.get().unwrap().is_none());
    }

    #[test]
    fn a_damaged_secret_file_is_an_error_not_a_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("phone.secret");
        for bad in ["", "abcd", &"zz".repeat(32), &"ab".repeat(33)] {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(FileStore::new(path.clone()).get().err(), Some(KeyError::Corrupt), "{bad:?}");
        }
    }

    #[test]
    fn hex_round_trip_and_rejects_bad_input() {
        let bytes = [0xab; 4];
        assert_eq!(hex_decode::<4>(&hex_encode(&bytes)), Some(bytes));
        assert_eq!(hex_decode::<4>("abc"), None);
        assert_eq!(hex_decode::<4>("zzzzzzzz"), None);
    }

    #[test]
    fn random_values_differ() {
        assert_ne!(random_bytes::<16>().unwrap(), random_bytes::<16>().unwrap());
    }
}
