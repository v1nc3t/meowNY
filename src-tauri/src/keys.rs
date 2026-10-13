//! Key handling. The database key is never stored as-is next to the data:
//!
//! * A random 32-byte secret lives in the OS keychain (or an equivalent secret service).
//! * Without a passphrase the secret is the database key.
//! * With a passphrase the key is HKDF(secret || Argon2id(passphrase, salt)), so the data file
//!   needs both the keychain entry and the passphrase. Copying the file elsewhere gets nothing.

use argon2::{Algorithm, Argon2, Params, Version};
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
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::Store => write!(f, "the OS keychain is not available"),
            KeyError::Random => write!(f, "no secure randomness available"),
            KeyError::Corrupt => write!(f, "the stored key is damaged"),
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

pub struct KeychainStore {
    entry: keyring::Entry,
}

impl KeychainStore {
    pub fn new() -> Result<Self, KeyError> {
        let entry = keyring::Entry::new("org.meowny.app", "database-key").map_err(|_| KeyError::Store)?;
        Ok(Self { entry })
    }
}

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

/// Derives the database key from the keychain secret and an optional passphrase.
pub fn derive_db_key(
    secret: &[u8; SECRET_LEN],
    passphrase: Option<&str>,
    salt: &[u8; SALT_LEN],
) -> Result<DbKey, KeyError> {
    let Some(passphrase) = passphrase else {
        return Ok(DbKey::from_bytes(*secret));
    };

    let mut stretched = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params())
        .hash_password_into(passphrase.as_bytes(), salt, stretched.as_mut())
        .map_err(|_| KeyError::Corrupt)?;

    let mut ikm = Zeroizing::new([0u8; SECRET_LEN + 32]);
    ikm[..SECRET_LEN].copy_from_slice(secret);
    ikm[SECRET_LEN..].copy_from_slice(stretched.as_ref());

    let mut okm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm.as_ref())
        .expand(b"meowny database key v1", &mut okm)
        .map_err(|_| KeyError::Corrupt)?;
    let key = DbKey::from_bytes(okm);
    okm.zeroize();
    Ok(key)
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

    fn key_bytes(key: &DbKey) -> String {
        // DbKey exposes the key only as its SQLCipher pragma value; compare through that.
        key.pragma_value_for_tests()
    }

    #[test]
    fn without_passphrase_the_secret_is_the_key() {
        let key = derive_db_key(&SECRET, None, &SALT).unwrap();
        assert_eq!(key_bytes(&key), key_bytes(&DbKey::from_bytes(SECRET)));
    }

    #[test]
    fn passphrase_changes_the_key() {
        let plain = derive_db_key(&SECRET, None, &SALT).unwrap();
        let a = derive_db_key(&SECRET, Some("correct horse"), &SALT).unwrap();
        let b = derive_db_key(&SECRET, Some("correct horse"), &SALT).unwrap();
        let c = derive_db_key(&SECRET, Some("wrong horse"), &SALT).unwrap();
        assert_eq!(key_bytes(&a), key_bytes(&b));
        assert_ne!(key_bytes(&a), key_bytes(&plain));
        assert_ne!(key_bytes(&a), key_bytes(&c));
    }

    #[test]
    fn key_depends_on_secret_and_salt() {
        let base = derive_db_key(&SECRET, Some("pw"), &SALT).unwrap();
        let other_secret = derive_db_key(&[8; 32], Some("pw"), &SALT).unwrap();
        let other_salt = derive_db_key(&SECRET, Some("pw"), &[1; 16]).unwrap();
        assert_ne!(key_bytes(&base), key_bytes(&other_secret));
        assert_ne!(key_bytes(&base), key_bytes(&other_salt));
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
