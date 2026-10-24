# Documents design

How receipts and invoices are stored, sealed, synced and backed up. Section numbers are cited in the code; keep them.
User side: the [guide](https://v1nc3t.github.io/meowNY/guide.html#documents).

## 1. What it has to do

A document is a file stored encrypted, with a name, a date and links to transactions, recurring items or ledger
entries. It lives on its own: deleting a transaction never deletes it. The desktop holds every file; the phone holds
those marked "keep on phone" and the ones it added until the desktop has them. Every file is untrusted. No new Rust
dependency. Not goals: editing, versions, OCR, thumbnails, folders, sharing.

## 2. The pieces

| Name | Where |
|---|---|
| Document row (name, date, type, size, SHA-256, keep on phone) | `documents` |
| Link row (one document to one target) | `document_links` |
| Sealed file | `documents/<name>.mnd` next to the database |
| Document key, derived from the database key | Memory only, dropped on lock |
| Plain copy for the system viewer | `open/`, removed on lock and close |

## 3. Keys and names

```
docs_root = HKDF-SHA256(salt = none, ikm = database key, info = "meowny documents v1")
file_key  = HKDF-SHA256(salt = file salt (16, in the header), ikm = docs_root, info = "meowny doc file v1" || sync_id (16))
file name = hex(HKDF-Expand(docs_root, "meowny doc name v1" || sync_id), 16 bytes) + ".mnd"
```

A random salt per file gives each file its own key. The sync id in the key means a file moved under another
document's name does not open. The name is derived, never stored. A new passphrase wraps the same database key, so
nothing is resealed.

## 4. The sealed file

```
header  = "MEOWNYDF" (8) || format u16 = 1 || chunk size u32 = 65536 || salt (16)          30 bytes
chunk_i = ChaCha20-Poly1305(file_key, nonce_i, aad = header, plaintext_i)
nonce_i = last (0x01 on the final chunk, else 0x00) || 0 0 0 || i (u64, big-endian)
```

A STREAM layout: a changed byte, a cut, an added, swapped or dropped chunk, or another document's file all fail.
Reads check size and SHA-256 against the row. Writes go to `name.tmp` and are renamed; incoming files build as
`name.part`, so transfers resume. At most 25 MiB per file.

## 5. The rows

Migration `0010_documents.sql` (`0011` fixed link markers). `documents`: `name` (1 to 120 cleaned characters),
`media_type` (a closed list: JPEG, PNG, GIF, WebP, PDF, text, HTML, CSV, octet-stream), `size_bytes`, `sha256`,
`document_date`, `keep_on_phone`. `document_links`: exactly one target (transaction, recurring item or, since phase
10, ledger entry), unique per document and target, `ON DELETE CASCADE`. Type, size and hash never change (a trigger).
A link marker means a person unlinked it. Whether a device holds a file is read from the folder, not a column.

## 6. Names and types: files are untrusted

- Names are cleaned: last part of a path, no control or hidden characters, at most 120 characters. Names from another
  device must already be clean. Copies on disk also avoid Windows-forbidden and reserved names.
- The type comes from the first bytes, never the extension. Images are drawn only as `data:` images of the four
  types, at most 64 megapixels and 10 MiB.
- An opened copy gets the extension of the type found, never the one in its name (`invoice.pdf.exe` opens as a PDF).
  Unknown types are never opened, only saved.

## 7. Adding, linking, deleting

The page sends a file to Rust as base64url pieces (`document_upload_begin`, `_chunk`, `_finish`), held in memory only;
no path ever comes from the page. Files dropped on the window are added by path in Rust (`documents/dropped.rs`).
Rust cleans the name, finds the type, hashes, writes the sealed file and the row together. The same hash stores
nothing new. Deleting a document removes the row first, the file after commit.

## 8. Rows and files that disagree

After every unlock, restore and sync: files no row names and stale `.tmp` files are removed; a row with a missing or
impossible file is counted, never repaired by guessing. Only names the app makes are touched. Nothing logs a name or hash.

## 9. Sync

Rows sync like any rows; the files' bytes follow after `done` in the same sealed connection (`sync/files.rs`,
protocol 3). Phone to desktop: every file the desktop lacks. Desktop to phone: only `keep_on_phone` files.

`have` and `need` lists, then per file a `file` message and `chunk` messages of up to 24 KiB, resealed under each
device's own key and checked against the hash. The phone drops a switched-off file only after the desktop confirms in
a `stored` message of that session that it holds it. At most 100 MiB of files per sync; the rest waits. A break in this
phase is a note, not a failed sync.

## 10. Merge rules for the rows

Newer stamp wins the whole row. The same link added on both becomes one. A link to a parent deleted elsewhere is
dropped with the parent. A known id arriving with another size, type, hash or target stops the sync. Two documents with
the same hash are both kept. Documents apply after transactions, and their markers last.

## 11. Backup, restore and "Delete all my data"

Backup format 2 is a STREAM of 64 KiB sealed chunks (as in section 4) holding records: `S` the snapshot, `F` a file
(`sync_id`, size, SHA-256, bytes), `E` the end. Documents are optional (on by default under 100 MB). A restore checks
the snapshot first, reseals each file under the new vault's key, replaces the database in one transaction, then runs
section 8. "Delete all my data" and the phone's erase remove `documents/` and `open/`.

## 12. Export

JSON, CSV and Excel list documents (name, date, type, size, hash, attachments) and never contain a file. A limited
export lists only documents attached to what it holds.

## 13. CLI and API

`documents list` and `show` (read token) never return contents. `add`, `update`, `link`, `unlink`, `delete` need a
write token; `add` sends the bytes in one body of at most 35 MiB, admitted only after the token is checked from the
request head. `get` needs the person's yes in the app and writes a new file whose extension matches the type found.

## 14. The screens

A documents page (search, type filter, sizes), details with "keep a copy on my phone", an attach area in the item
dialogs, a paper clip with a count on rows. Preview draws images, PDF (`pdfjs-dist`, the one added dependency, no
`eval` or wasm), text, CSV, and Word and Excel through the app's own hardened zip and XML readers (`ui/src/lib/preview/`).

## 15. The spike on a real phone

Done 2026-10-23 on the maintainer's phone: adding from the chooser and the camera, large files, previews, opening,
the "keep on phone" switch through a sync, and `open/` emptied on lock.

## 16. Open points

Thumbnails (a second sealed file per image), a size limit or warning for the phone, removal of an open copy on Windows
while a viewer holds it, and an outside review of the sealed format and the file transfer.

## 17. Tests

Known answers from a second implementation (`scripts/doc-vectors.py`, now in git history) for keys, names and files; every kind of damage
refused; hostile names and lying types; row constraints and triggers; the agreement check; every merge rule and random
edits on two devices; transfers cut at every chunk; backups with and without documents; `open/` empty after a lock;
API tokens that must not see contents.
