//! Files, when the app is served to a browser by `meowny-server` instead of shown in a window.
//!
//! The commands that save or read a file ask the person for a place with a file dialog (`commands::ask`). A server has no window
//! to show one in, so while this is installed the dialogs are answered here instead:
//! - a save goes to a new file in a staging folder, which the server then hands to the browser as a download;
//! - a pick is the file the browser uploaded just before the command, or none (as if the person cancelled);
//! - a folder (scheduled backups) is the backup folder the server was started with.
//!
//! The server runs one file command at a time (`begin` .. `finish`), so the one exchange below is that command's. A save or pick
//! outside of `begin` .. `finish` is refused rather than written somewhere nobody fetches it from.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::vault::AppError;

/// A file a command wrote for the person, and the name the save dialog would have suggested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub path: PathBuf,
    pub name: String,
}

struct Exchange {
    staging: PathBuf,
    backups: PathBuf,
    /// Inside `begin` .. `finish`.
    active: bool,
    upload: Option<PathBuf>,
    download: Option<Download>,
}

static EXCHANGE: Mutex<Option<Exchange>> = Mutex::new(None);

fn exchange() -> std::sync::MutexGuard<'static, Option<Exchange>> {
    EXCHANGE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turns the file dialogs into the exchange. `staging` holds the files on their way to the browser; `backups` is where scheduled
/// backups go. Both must exist.
pub fn install(staging: PathBuf, backups: PathBuf) {
    *exchange() = Some(Exchange { staging, backups, active: false, upload: None, download: None });
}

/// True in `meowny-server`.
pub fn is_served() -> bool {
    exchange().is_some()
}

/// Starts one file command. `upload` is the file the browser sent for it, if any.
pub fn begin(upload: Option<PathBuf>) {
    if let Some(ex) = exchange().as_mut() {
        ex.active = true;
        ex.upload = upload;
        ex.download = None;
    }
}

/// Ends the file command and returns the file it wrote for the browser, if it wrote one.
pub fn finish() -> Option<Download> {
    let mut guard = exchange();
    let ex = guard.as_mut()?;
    ex.active = false;
    ex.upload = None;
    ex.download.take().filter(|d| d.path.is_file())
}

/// In place of a save dialog: a new path in the staging folder. `name` is what the browser will offer to call it.
pub(crate) fn save(name: &str) -> Result<Option<PathBuf>, AppError> {
    let mut guard = exchange();
    let ex = guard.as_mut().filter(|ex| ex.active).ok_or(AppError::InvalidInput)?;
    let path = ex.staging.join(random_name()?);
    ex.download = Some(Download { path: path.clone(), name: name.to_string() });
    Ok(Some(path))
}

/// In place of an open dialog: the uploaded file, or none when the browser sent none (the person cancelled).
pub(crate) fn pick() -> Result<Option<PathBuf>, AppError> {
    let mut guard = exchange();
    let ex = guard.as_mut().filter(|ex| ex.active).ok_or(AppError::InvalidInput)?;
    Ok(ex.upload.take())
}

/// In place of a folder dialog: the server's backup folder.
pub(crate) fn folder() -> Result<Option<PathBuf>, AppError> {
    let guard = exchange();
    let ex = guard.as_ref().ok_or(AppError::InvalidInput)?;
    Ok(Some(ex.backups.clone()))
}

/// 128 random bits as hex: a name nobody can guess, for a staged file or a session.
pub fn random_name() -> Result<String, AppError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| AppError::Internal)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Where the staging folder is, for a test.
#[cfg(test)]
fn staging() -> Option<PathBuf> {
    exchange().as_ref().map(|ex| ex.staging.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // One test, since the exchange is global to the process.
    #[test]
    fn the_exchange_answers_only_inside_a_command() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        std::fs::create_dir(&backups).unwrap();
        install(dir.path().to_path_buf(), backups.clone());
        assert!(is_served());
        assert_eq!(staging().as_deref(), Some(dir.path()));

        // outside a command nothing is answered
        assert_eq!(save("a.json"), Err(AppError::InvalidInput));
        assert_eq!(pick(), Err(AppError::InvalidInput));
        assert_eq!(folder(), Ok(Some(backups.clone())));

        // a save is staged and handed back once written
        begin(None);
        let path = save("a.json").unwrap().unwrap();
        assert!(path.starts_with(dir.path()));
        assert_eq!(pick(), Ok(None));
        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(finish(), Some(Download { path: path.clone(), name: "a.json".into() }));
        assert_eq!(save("b.json"), Err(AppError::InvalidInput));

        // a save that wrote nothing (the command failed after asking) hands nothing back
        begin(None);
        save("c.json").unwrap();
        assert_eq!(finish(), None);

        // an upload is picked once
        let sent = dir.path().join("sent");
        begin(Some(sent.clone()));
        assert_eq!(pick(), Ok(Some(sent)));
        assert_eq!(pick(), Ok(None));
        assert_eq!(finish(), None);
        assert!(Path::new(&random_name().unwrap()).as_os_str().len() == 32);
    }
}
