//! Files dropped on the window (`docs/documents-design.md`, section 7).
//!
//! The web view cannot be relied on to hand a page the files that are dropped on it: on Linux, WebKitGTK gives the page an empty list.
//! So the window takes the drop itself, which is told where the files are. The page still never names a path: Rust remembers what was
//! dropped (only while the app is unlocked), tells the page that something was, and the page asks to add what was dropped. Nothing but
//! what the person dragged onto the window can be read this way, and a lock forgets it.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use zeroize::Zeroizing;

use super::library::{AddOutcome, NewDocument};
use super::names::clean_file_name;
use super::seal::MAX_DOCUMENT_BYTES;
use crate::vault::{AppError, Vault, VaultStatus};

/// More files than this in one drop are left out, and the answer says how many.
pub const MAX_PER_DROP: usize = 100;

/// What was dropped and not added yet.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub paths: Vec<PathBuf>,
    /// How many files past `MAX_PER_DROP` were left out.
    pub ignored: u32,
}

#[derive(Default)]
pub struct Dropped(Mutex<Pending>);

impl Dropped {
    fn slot(&self) -> std::sync::MutexGuard<'_, Pending> {
        // The slot is only ever swapped whole, so a poisoned lock still holds valid data.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Remembers what was just dropped, in place of what was dropped before.
    pub fn set(&self, mut paths: Vec<PathBuf>) {
        let ignored = paths.len().saturating_sub(MAX_PER_DROP) as u32;
        paths.truncate(MAX_PER_DROP);
        *self.slot() = Pending { paths, ignored };
    }

    /// Hands over what was dropped, once.
    pub fn take(&self) -> Pending {
        std::mem::take(&mut *self.slot())
    }

    pub fn clear(&self) {
        *self.slot() = Pending::default();
    }

    pub fn is_empty(&self) -> bool {
        self.slot().paths.is_empty()
    }
}

/// What the page is told about one dropped file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOutcome {
    /// The name of the file as the library would keep it (never a path).
    pub name: String,
    /// `added`, `alreadyThere` or `failed`.
    pub status: &'static str,
    /// Why, when it failed: one of the fixed codes the screens have words for.
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroppedResult {
    pub outcomes: Vec<FileOutcome>,
    /// Files left out because a drop holds at most `MAX_PER_DROP`.
    pub ignored: u32,
}

/// The bytes of a dropped file. Only a regular file of 1 byte to 25 MiB; a folder, an empty file and a file that is not there any more
/// are refused here, and a file bigger than the limit before it is read.
fn read(path: &Path) -> Result<Zeroizing<Vec<u8>>, AppError> {
    let meta = fs::metadata(path).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { AppError::NotFound } else { AppError::Internal })?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(AppError::InvalidInput);
    }
    if meta.len() > MAX_DOCUMENT_BYTES {
        return Err(AppError::TooLarge);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(meta.len() as usize));
    fs::File::open(path).and_then(|f| f.take(MAX_DOCUMENT_BYTES + 1).read_to_end(&mut bytes)).map_err(|_| AppError::Internal)?;
    // a file that grew while it was read
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(AppError::TooLarge);
    }
    Ok(bytes)
}

/// Adds each dropped file as a document of its own, named after the file, dated today, with no attachments. `add` stores one: the
/// app passes the library's own function, so everything the screens check holds here too. A file that cannot be added is reported and
/// the rest go on.
pub fn add_all(pending: Pending, mut add: impl FnMut(&NewDocument, &[u8]) -> Result<AddOutcome, AppError>) -> DroppedResult {
    let mut outcomes = Vec::new();
    for path in &pending.paths {
        let raw = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = clean_file_name(&raw);
        let input = NewDocument { name: raw, ..Default::default() };
        let outcome = read(path).and_then(|bytes| add(&input, &bytes));
        outcomes.push(match outcome {
            Ok(done) if done.already_there => FileOutcome { name, status: "alreadyThere", error: None },
            Ok(_) => FileOutcome { name, status: "added", error: None },
            Err(error) => FileOutcome { name, status: "failed", error: Some(error) },
        });
    }
    DroppedResult { outcomes, ignored: pending.ignored }
}

/// What a drag over the window means for the app: the events to send to the page, as a name and a payload. While something is held over
/// the window the page is told (`documents-drag`, true) so it can show where to drop, and when it is dropped the app remembers the files
/// and tells the page (`documents-dropped`, how many). A locked app takes nothing.
pub fn on_drag(vault: &Vault, event: &tauri::DragDropEvent) -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    use tauri::DragDropEvent;
    let unlocked = vault.status() == VaultStatus::Unlocked;
    match event {
        DragDropEvent::Enter { paths, .. } if unlocked && !paths.is_empty() => vec![("documents-drag", json!(true))],
        DragDropEvent::Drop { paths, .. } if unlocked && !paths.is_empty() => {
            vault.dropped().set(paths.clone());
            vec![("documents-drag", json!(false)), ("documents-dropped", json!(paths.len()))]
        }
        DragDropEvent::Leave | DragDropEvent::Drop { .. } => vec![("documents-drag", json!(false))],
        _ => Vec::new(),
    }
}
