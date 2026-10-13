//! The text file a person can save their recovery key to.

use std::path::Path;

use crate::keys::RecoveryKey;
use crate::vault::AppError;

/// The file contents for a recovery key. Refuses anything that is not a valid key, and
/// writes the key in its standard form, so nothing else can be smuggled into the file.
pub fn text(code: &str) -> Result<String, AppError> {
    let key = RecoveryKey::parse(code).ok_or(AppError::InvalidInput)?;
    Ok(format!(
        "meowny recovery key\n\n{}\n\n\
         Keep this file somewhere safe, away from this computer.\n\
         It is the only way back in if you forget your passphrase.\n\
         Anyone who has this key and your meowny data folder can open your data.\n\
         Making a new recovery key in Settings stops this one working.\n",
        key.to_display()
    ))
}

/// Writes the file readable by the owner only.
pub fn write(path: &Path, text: &str) -> Result<(), AppError> {
    std::fs::write(path, text).map_err(|_| AppError::Internal)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|_| AppError::Internal)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_holds_the_key_in_standard_form() {
        let shown = RecoveryKey::generate().unwrap().to_display();
        let sloppy = shown.to_lowercase().replace('-', " ");
        let file = text(&sloppy).unwrap();
        assert!(file.contains(&shown));
        assert!(file.starts_with("meowny recovery key"));
    }

    #[test]
    fn anything_that_is_not_a_recovery_key_is_refused() {
        for bad in ["", "hello", "my passphrase is hunter2 and a bit more", "XXXX-XXXX"] {
            assert_eq!(text(bad), Err(AppError::InvalidInput), "{bad:?}");
        }
    }

    #[test]
    fn extra_text_around_a_key_is_refused() {
        let shown = RecoveryKey::generate().unwrap().to_display();
        assert_eq!(text(&format!("{shown}\nrm -rf")), Err(AppError::InvalidInput));
    }

    #[test]
    fn writing_creates_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.txt");
        let shown = RecoveryKey::generate().unwrap().to_display();
        write(&path, &text(&shown).unwrap()).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains(&shown));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
