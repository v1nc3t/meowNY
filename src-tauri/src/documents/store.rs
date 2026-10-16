//! The folder of sealed files (`docs/documents-design.md`, sections 4 and 8).
//!
//! `documents/` sits next to the database. A document's file is `<stem>.mnd`, where the stem is worked out from the document's
//! id and the key (`DocKeys::stem`), so nothing has to store where a file is and "does this device hold it" is a look at the
//! folder. A file is written under a `.tmp` name and renamed when it is whole; a file that is arriving from the other device is
//! a `.part` that holds a valid beginning and can be carried on. The store knows nothing of the database: callers pass the ids
//! and sizes they want to check against.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::seal::{
    read_prefix, sealed_len, DocError, DocId, DocKeys, Opener, Sealer, CHUNK_SIZE, HEADER_LEN, MAX_DOCUMENT_BYTES, SEALED_CHUNK,
};

/// The folder's name inside the data folder.
pub const DIR_NAME: &str = "documents";

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub struct Store {
    dir: PathBuf,
}

/// What `check` found and did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Check {
    /// Files that no row names, removed.
    pub strays_removed: usize,
    /// Half-written files from a crash, removed.
    pub temp_removed: usize,
    /// Unfinished incoming files that no row names, removed.
    pub parts_removed: usize,
    /// Unfinished incoming files of rows that exist, kept so the transfer can carry on.
    pub parts_kept: usize,
    /// Things in the folder that are not ours (another name, a folder, a link). Left alone.
    pub unknown: usize,
    /// Rows with no file here.
    pub missing: Vec<DocId>,
    /// Rows whose file has a length that its row cannot have. Reported, not touched.
    pub damaged: Vec<DocId>,
}

impl Store {
    pub fn new(data_dir: &Path) -> Store {
        Store { dir: data_dir.join(DIR_NAME) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn ensure_dir(&self) -> Result<(), DocError> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&self.dir)?;
        Ok(())
    }

    fn path(&self, keys: &DocKeys, id: &DocId, extension: &str) -> PathBuf {
        self.dir.join(format!("{}.{extension}", keys.stem(id)))
    }

    /// Whether this device holds a file for the document. It does not open it.
    pub fn has(&self, keys: &DocKeys, id: &DocId) -> bool {
        fs::symlink_metadata(self.path(keys, id, "mnd")).is_ok_and(|m| m.is_file())
    }

    /// The stems of every finished file in the folder, for a list that asks about many documents at once.
    pub fn present_stems(&self) -> Result<HashSet<String>, DocError> {
        let mut stems = HashSet::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(stems),
            Err(e) => return Err(e.into()),
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if let Some((stem, "mnd")) = entry.file_name().to_str().and_then(split_name) {
                stems.insert(stem.to_string());
            }
        }
        Ok(stems)
    }

    /// Seals `bytes` into the folder. Nothing is there under the final name until the whole file is written and flushed.
    pub fn put(&self, keys: &DocKeys, id: &DocId, bytes: &[u8]) -> Result<(), DocError> {
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(DocError::TooLarge);
        }
        self.ensure_dir()?;
        let temp = self.path(keys, id, "tmp");
        let result = (|| {
            let mut sealer = Sealer::new(create_private(&temp)?, keys, id, bytes.len() as u64)?;
            sealer.write(bytes)?;
            let file = sealer.finish()?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, self.path(keys, id, "mnd"))?;
            sync_dir(&self.dir);
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    /// Opens a file and proves it is the one its row describes: every chunk opens, and the size and the SHA-256 are the
    /// row's. Only then are the bytes handed over.
    pub fn get(&self, keys: &DocKeys, id: &DocId, size: u64, sha256: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>, DocError> {
        let path = self.path(keys, id, "mnd");
        let file = File::open(&path).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { DocError::Missing } else { e.into() })?;
        let len = file.metadata()?.len();
        let mut opener = Opener::new(file, len, keys, id)?;
        if opener.plaintext_len() != size {
            return Err(DocError::Mismatch);
        }
        let mut out = Zeroizing::new(Vec::with_capacity(size as usize));
        let mut hasher = Sha256::new();
        while let Some(chunk) = opener.next_chunk()? {
            hasher.update(&*chunk);
            out.extend_from_slice(&chunk);
        }
        if out.len() as u64 != size || hasher.finalize().as_slice() != sha256 {
            return Err(DocError::Mismatch);
        }
        Ok(out)
    }

    /// Removes everything held for a document: the file, a half-written one and an unfinished incoming one. A file that is
    /// not there is not an error.
    pub fn remove(&self, keys: &DocKeys, id: &DocId) -> Result<(), DocError> {
        let mut failed = None;
        for extension in ["mnd", "tmp", "part"] {
            match fs::remove_file(self.path(keys, id, extension)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => failed = Some(e.kind()),
            }
        }
        match failed {
            Some(kind) => Err(DocError::Io(kind)),
            None => Ok(()),
        }
    }

    /// Removes the whole folder ("delete all my data", a phone that erases itself).
    pub fn remove_all(&self) -> Result<(), DocError> {
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Starts receiving a file of `size` bytes from the other device, or carries on with the unfinished one. The sender must
    /// start at `Incoming::offset`: that many bytes are already here, sealed and checked.
    pub fn begin_incoming(&self, keys: &DocKeys, id: &DocId, size: u64) -> Result<Incoming, DocError> {
        if size > MAX_DOCUMENT_BYTES {
            return Err(DocError::TooLarge);
        }
        self.ensure_dir()?;
        let part = self.path(keys, id, "part");
        let dest = self.path(keys, id, "mnd");

        if let Some((file, hasher, sealer_parts)) = self.resume(&part, keys, id, size)? {
            let (header, chunks) = sealer_parts;
            let sealer = Sealer::resume(file, keys, id, size, &header, chunks)?;
            let offset = chunks * CHUNK_SIZE as u64;
            return Ok(Incoming { sealer, hasher, size, received: offset, offset, part, dest, dir: self.dir.clone() });
        }

        let sealer = Sealer::new(create_private(&part)?, keys, id, size)?;
        Ok(Incoming { sealer, hasher: Sha256::new(), size, received: 0, offset: 0, part, dest, dir: self.dir.clone() })
    }

    /// Looks at an unfinished file. If it has a valid beginning that leaves something to receive, cuts it back to whole chunks
    /// and returns it ready to be written to, with the hash of what it holds. Otherwise removes it.
    #[allow(clippy::type_complexity)]
    fn resume(&self, part: &Path, keys: &DocKeys, id: &DocId, size: u64) -> Result<Option<(File, Sha256, ([u8; HEADER_LEN], u64))>, DocError> {
        let len = match fs::metadata(part) {
            Ok(m) if m.is_file() => m.len(),
            Ok(_) => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut hasher = Sha256::new();
        let scanned = read_prefix(File::open(part)?, len, keys, id, |plain| hasher.update(plain));
        match scanned {
            Ok((header, chunks)) if chunks * (CHUNK_SIZE as u64) < size => {
                let mut file = OpenOptions::new().write(true).open(part)?;
                file.set_len(HEADER_LEN as u64 + chunks * SEALED_CHUNK as u64)?;
                file.seek(SeekFrom::End(0))?;
                Ok(Some((file, hasher, (header, chunks))))
            }
            _ => {
                let _ = fs::remove_file(part);
                Ok(None)
            }
        }
    }

    /// Throws away an unfinished file of this document (and a half-written one), and leaves a finished one alone. For a writer that starts
    /// from the first byte and cannot use what an earlier transfer left.
    pub fn discard_unfinished(&self, keys: &DocKeys, id: &DocId) {
        for extension in ["part", "tmp"] {
            let _ = fs::remove_file(self.path(keys, id, extension));
        }
    }

    /// How many bytes of the file of this document are already here as the good start of an unfinished transfer, which is where the
    /// next one can begin (a multiple of the chunk size, and less than `size`), or 0. Only looks: nothing is created, cut or removed.
    pub fn resume_offset(&self, keys: &DocKeys, id: &DocId, size: u64) -> u64 {
        let part = self.path(keys, id, "part");
        let Ok(meta) = fs::metadata(&part) else { return 0 };
        let Ok(file) = File::open(&part) else { return 0 };
        match read_prefix(file, meta.len(), keys, id, |_| {}) {
            Ok((_, chunks)) if meta.is_file() && chunks * (CHUNK_SIZE as u64) < size => chunks * CHUNK_SIZE as u64,
            _ => 0,
        }
    }

    /// Compares the folder with the rows: `docs` is every document with its size. Removes what no row names and what a crash
    /// left, and reports the rows that have no file or a file of the wrong length. Only things that are ours are touched.
    pub fn check(&self, keys: &DocKeys, docs: &[(DocId, u64)]) -> Result<Check, DocError> {
        let expected: HashMap<String, u64> = docs.iter().map(|(id, size)| (keys.stem(id), *size)).collect();
        let mut report = Check::default();
        let mut present = HashSet::new();
        let mut wrong_size = HashSet::new();

        match fs::read_dir(&self.dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let parsed = name.to_str().and_then(split_name);
                    let (Some((stem, extension)), true) = (parsed, entry.file_type().is_ok_and(|t| t.is_file())) else {
                        report.unknown += 1;
                        continue;
                    };
                    let known = expected.get(stem);
                    match (extension, known) {
                        ("mnd", Some(size)) => {
                            present.insert(stem.to_string());
                            if entry.metadata().is_ok_and(|m| m.len() != sealed_len(*size)) {
                                wrong_size.insert(stem.to_string());
                            }
                        }
                        ("mnd", None) => report.strays_removed += fs::remove_file(entry.path()).is_ok() as usize,
                        ("tmp", _) => report.temp_removed += fs::remove_file(entry.path()).is_ok() as usize,
                        ("part", Some(_)) => report.parts_kept += 1,
                        ("part", None) => report.parts_removed += fs::remove_file(entry.path()).is_ok() as usize,
                        _ => report.unknown += 1,
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        for (id, _) in docs {
            let stem = keys.stem(id);
            if !present.contains(&stem) {
                report.missing.push(*id);
            } else if wrong_size.contains(&stem) {
                report.damaged.push(*id);
            }
        }
        Ok(report)
    }
}

/// A file arriving from the other device. Bytes are sealed into the `.part` file one chunk at a time, so what is on disk is
/// always a valid beginning; `finish` checks the hash and only then gives the file its final name.
pub struct Incoming {
    sealer: Sealer<File>,
    hasher: Sha256,
    size: u64,
    received: u64,
    offset: u64,
    part: PathBuf,
    dest: PathBuf,
    dir: PathBuf,
}

impl Incoming {
    /// Where the sender has to start: the bytes before this are here already.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// How many bytes of the file are here, including those that wait for a whole chunk.
    pub fn received(&self) -> u64 {
        self.received
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), DocError> {
        self.sealer.write(data)?;
        self.hasher.update(data);
        self.received += data.len() as u64;
        Ok(())
    }

    /// Gives the file its name if everything is here and hashes to `sha256`. A file with another hash is removed. If bytes
    /// are missing the unfinished file stays, so a later transfer can carry on.
    pub fn finish(self, sha256: &[u8; 32]) -> Result<(), DocError> {
        if self.received != self.size {
            return Err(DocError::Incomplete);
        }
        let file = self.sealer.finish()?;
        file.sync_all()?;
        drop(file);
        if self.hasher.finalize().as_slice() != sha256 {
            let _ = fs::remove_file(&self.part);
            return Err(DocError::Mismatch);
        }
        fs::rename(&self.part, &self.dest)?;
        sync_dir(&self.dir);
        Ok(())
    }
}

/// `<32 lowercase hex>.<extension>`, the only names the store makes.
fn split_name(name: &str) -> Option<(&str, &str)> {
    let (stem, extension) = name.split_once('.')?;
    let hex = stem.len() == 32 && stem.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    hex.then_some((stem, extension))
}

fn create_private(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Makes the rename itself durable. Nothing depends on it for safety (a file that is not there is reported as missing and
/// brought back by a sync), so a failure is not an error.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}
