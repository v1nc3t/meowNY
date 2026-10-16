//! The copies handed to the system's viewer (`docs/documents-design.md`, section 6).
//!
//! A PDF or an HTML receipt is shown by a program of the system, which needs a plain file to open. The file is made in `open/`
//! next to the database, in a folder of its own with a random name, readable by its owner only, and the whole folder is emptied when
//! the app locks, when it exits, when it starts again (a crash leaves files behind) and on "delete all my data". While a copy
//! exists it is not encrypted; the screen says so before the first one is opened.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::keys;

use super::seal::DocError;

/// The folder's name inside the data folder.
pub const DIR_NAME: &str = "open";

pub struct Opened {
    dir: PathBuf,
}

impl Opened {
    pub fn new(data_dir: &Path) -> Opened {
        Opened { dir: data_dir.join(DIR_NAME) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Removes every copy. Says whether the folder is gone: on Windows a viewer that still holds a file open keeps it, and the
    /// next start tries again.
    pub fn clear(&self) -> bool {
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    }

    /// Writes `bytes` as a new file called `name` in a new folder of its own, and returns where it is. The name is the one
    /// `names::open_name` made; anything with a separator in it, or a name that is not one, is refused, so the file can only
    /// land in the folder that was made for it.
    pub fn make_copy(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, DocError> {
        if name.is_empty() || name == "." || name == ".." || name.len() > 255 || name.contains(['/', '\\', '\0']) {
            return Err(DocError::Format);
        }
        make_private_dir(&self.dir)?;
        let folder = self.dir.join(keys::hex_encode(&keys::random_bytes::<16>()?));
        make_private_dir(&folder)?;
        let path = folder.join(name);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = options.open(&path).and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()));
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&folder);
            return Err(e.into());
        }
        Ok(path)
    }
}

fn make_private_dir(dir: &Path) -> Result<(), DocError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    Ok(())
}
