# Documents design (phase 9)

Status: built (batches 1 to 11). Decisions come from phase 9 of [PLAN.md](PLAN.md). Section 16 lists open points; the real-phone check (section 15) settles part of them.

## 1. What it has to do

- Keep the paper trail next to the numbers. A document is a file stored encrypted on the device, with a name, a date and links to transactions and recurring items.
- It exists on its own and links to any number of items. Deleting a transaction never deletes a document.
- The desktop holds every file. The phone holds only files marked "keep on phone" (off when added) and ones it added itself, until the desktop has them.
- Both devices add, link, rename and delete. The phone adds from the camera or a file.
- Every file is untrusted: nothing is run, nothing is shown with scripts.
- No new Rust dependency (`chacha20poly1305`, `hkdf`, `sha2`, `zeroize`, `getrandom`, `std`).

Not goals: editing a file, versions, OCR, thumbnails, folders or tags, sharing with another person, a total size limit.

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Document row | Name, date, type, size, SHA-256, "keep on phone" | Encrypted database (`documents`) |
| Link row | One document linked to one transaction or recurring item | Encrypted database (`document_links`) |
| Sealed file | The contents, encrypted in chunks | `documents/` next to the database |
| Document key | Derived from the database key; memory only while unlocked | Vault state, dropped by a lock |
| Open copy | A plain copy for the system's viewer | `open/`, removed on lock and close |

Files are not blobs in the database: ten years of receipts would make it large and slow.

## 3. Keys and names

```
docs_root  = HKDF-SHA256(salt = none, ikm = database key, info = "meowny documents v1")          32 bytes
file_key   = HKDF-SHA256(salt = file salt (16, in the file header), ikm = docs_root,
                         info = "meowny doc file v1" || sync_id (16 bytes))                         32 bytes
file name  = hex(HKDF-Expand(docs_root, info = "meowny doc name v1" || sync_id), 16 bytes) + ".mnd"
```

- `docs_root` is made when the vault opens (`unlock`, `create`, `recover`), held as `Zeroizing` next to the connection, and dropped by every lock.
- A random salt per file gives each file its own key, so counter nonces never repeat under a key.
- The sync id is in the key: a sealed file moved to another document's name does not open.
- The name is derived, not stored: no column can fall out of step. The folder still shows how many files and about how big (stated in the data inventory).
- A changed passphrase or recovery key wraps the same database key, so no file is resealed. A restore into a new vault reseals files as it reads them (section 11).

## 4. The sealed file

```
file    = header || chunk_0 || chunk_1 || ... || chunk_n
header  = "MEOWNYDF" (8) || format u16 = 1 (2) || chunk size u32 = 65536 (4) || salt (16)        30 bytes
chunk_i = ChaCha20-Poly1305(file_key, nonce_i, aad = header, plaintext_i)                         16 bytes more than the plaintext
nonce_i = last (1: 0x01 on the final chunk, else 0x00) || 0 0 0 || i (u64, big-endian)           12 bytes
```

STREAM layout. Every chunk but the last holds exactly 65,536 bytes; the last holds 1 to 65,536 (an empty file is one last chunk of 0 bytes). The chunk count comes from the file length.

| Damage | Why it is caught |
|---|---|
| A changed byte anywhere | That chunk's tag fails (header is the aad) |
| A chunk cut off the end | The new last chunk was sealed without the flag |
| A chunk added at the end | The old last chunk was sealed with the flag |
| Chunks swapped or dropped | The counter is in the nonce |
| Another document's file under this name | The sync id is in the key |

- An empty last chunk after other chunks is refused. The reader checks size and SHA-256 against the row before handing anything over; failure means "damaged".
- **Writing** is atomic: `name.tmp`, flush, rename (folder flushed on Unix), owner-only. An incoming file is built as `name.part` (a valid prefix, all chunks non-final) and renamed after the hash matches, which makes transfers resumable.
- **Limits.** 25 MiB per file (`MAX_DOCUMENT_BYTES`, 400 chunks). Rows with size 0 or over the limit are refused from other devices. A person adding an empty file is refused; the format handles it.

## 5. The rows

Migration `0010_documents.sql`. Dates are `YYYY-MM-DD`.

```
documents (
  id, sync_id, stamp,
  name           text   1..120 characters, cleaned (section 6)
  media_type     text   one of a closed list (below)
  size_bytes     int    1..26214400
  sha256         text   64 lowercase hex
  document_date  text   a real date, the day of the paper (default: today in the person's time zone)
  keep_on_phone  int    0 or 1, default 0
  created_at, updated_at
)

document_links (
  id, sync_id, stamp,
  document_id               -> documents      ON DELETE CASCADE
  transaction_id            -> transactions   ON DELETE CASCADE, may be empty
  recurring_transaction_id  -> recurring_transactions  ON DELETE CASCADE, may be empty
  created_at
  exactly one of the two targets is set (CHECK)
  unique (document_id, transaction_id) and unique (document_id, recurring_transaction_id), each where set
)
```

- **Immutable contents:** `media_type`, `size_bytes`, `sha256` never change; to change a file, delete and add another. Enforced by a `BEFORE UPDATE` trigger (also for `sync_id` and every link column; setting a column to its current value is allowed).
- **Media types** (closed list, from the first bytes): `image/jpeg`, `image/png`, `image/gif`, `image/webp`, `application/pdf`, `text/plain`, `text/html`, `text/csv`, `application/octet-stream`.
- Real foreign keys keep links consistent. Phase 10 added a third target (a ledger entry) by rebuilding the table.
- Audit records document and link changes with the name, never the contents.
- `sync_deleted.kind` gets `documents` and `document_links` (table rebuilt). `backup_schedule` gets `include_documents`. Both tables are in backups.
- Stamp triggers, `applying` and markers follow `0008`/`0009`; `sync_id` is a column default (`lower(hex(randomblob(16)))`); the `documents` update trigger fires only when `name`, `document_date` or `keep_on_phone` really changes.
- **A link marker means a person unlinked it** (migration `0011`; `0010` wrongly made one for every removal). A link removed by foreign key needs none: the parent's marker says it. The trigger tells the two apart by whether the parent still exists. `0011` also fixed the `documents` `updated_at` trigger to ignore no-op writes (found by the random sync test).

**Does this device hold the file?** Look at the folder: the name is derived, so check `documents/<name>.mnd`. No column. A row with no file shows:

| State | Where | Meaning |
|---|---|---|
| Not on the phone | phone, switch off | Normal |
| Waiting for sync | phone with switch on, or either device after a stopped transfer | The next sync brings it |
| Missing | desktop | The folder lost it; the phone may hold it and the next sync takes it back |

## 6. Names and types: files are untrusted

- **Names** are cleaned on add or rename.
  - From a file (`clean_file_name`): last part after `/` or `\`; control and hidden characters removed; white space folded to one plain space; cut at 120 characters keeping a short extension; empty or only dots becomes `document`.
  - From typing (`clean_title`): `/` or `\` becomes a hyphen; an empty result is refused.
  - A name arriving from another device must already be clean (`is_clean_name`) or the row is refused.
- **A copy on disk** also goes through `disk_name()`: Windows-forbidden characters (`<>:"|?*`) become `_`, trailing dots and spaces go, reserved names (`CON`, `NUL`, `COM1`, ...) get a leading underscore.
- **Type** comes from the first bytes (`%PDF-`, PNG/JPEG/GIF/WebP signatures, UTF-8 text with no NUL); the extension only tells text from CSV and HTML. A lying `.jpg` is `text/plain` or `application/octet-stream` and never drawn. An image also needs a header giving width and height.
- **Images in the app:** only the four image types, at most 64 megapixels (from the header) and 10 MiB. Bytes go as base64url text; the page draws a `data:` `<img>`, never SVG, a frame or a link. The content policy already allows `data:` images. Bigger images open in the system viewer.
- **Everything else with a kind** goes to `open/` and the system viewer (`tauri-plugin-opener`, called from Rust so the page names no path).
  - **The copy's extension comes from the kind the app found, never the name** (`open_name`: `invoice.pdf.exe` found to be a PDF opens as `invoice.pdf.pdf`). Otherwise "open" could run a program.
  - `application/octet-stream` is never opened from the app, only saved as a copy.
  - Before the first open of a session the screen says the copy is not encrypted and that HTML may load things from the internet.
- **`open/`:** owner-only, one random-named sub-folder per open. Emptied on lock, exit, start (a crash leaves files) and "delete all my data". On Windows a viewer holding the file blocks removal until the next start.
- **Saving a copy** (desktop only) uses `commands::ask`, owner-only, never over an existing file, with the export warning.

## 7. Adding, linking, deleting

**Adding** (one Rust function for both apps):

1. The page reads a chosen file (`File.arrayBuffer()`; phone: `capture` for the camera) and sends it to Rust **as text pieces**: `document_upload_begin` (name, date, links, size), `document_upload_chunk` (base64url, 256 KiB of file each), `document_upload_finish`.
   - Tauri does not support raw bytes on Android, where photos come from.
   - Pieces are held **in memory only**, one upload at a time; a lock or a new upload drops it.
   - A bad description is refused at `begin`; an out-of-order piece or one past the size ends the upload. Over 25 MiB is refused in the page and in Rust. No path ever comes from the page.
   - **Dropping files on the window** (desktop): on Linux WebKitGTK gives the page an empty list, so the window takes the drop. `documents/dropped.rs` keeps the paths in the vault (at most 100, forgotten on lock), emits `documents-drag` and `documents-dropped`, and the page calls `document_add_dropped` (no argument). Rust adds exactly those files (regular, 1 byte to 25 MiB; others are reported by name and the rest go on), dated today, attached to nothing.
2. Rust cleans the name, decides the type, hashes, and in one `with_conn` call writes the sealed file and inserts the row (and links).
3. A document with the same SHA-256 stores nothing: the answer says "already in the library", links go to the existing one, and a lost file is restored. Duplicates made on two devices while apart are kept.

- If the row insert fails, the file is removed; if the process dies between, the section 8 check removes the stray.
- **Linking** adds a row (an existing link is not an error). **Unlinking** deletes it and leaves a marker. Deleting a parent removes its links by foreign key; a document is never touched.
- **Deleting a document** confirms, naming it and its link count. Row first inside the transaction, file after commit (a file without a row is cleaned; a row without a file is the worse state).
- **Attach area:** lists linked documents (open, unlink), "add file" (phone: "take photo"), "link an existing document" (library search). In a creating dialog, picked files wait and are added after the transaction exists; a failure names the file.
- **Errors** (fixed codes, seven languages): `too_large` (API 413), `file_missing` (404), `file_damaged` (500 `internal`). A refused add leaves no row, link or file.

## 8. Rows and files that disagree

A check runs after every unlock, restore and sync:

- A file whose name no row derives is removed; `.tmp` files are removed; `.part` files stay when their row exists.
- A row whose file has an impossible length is reported damaged and left. The check reads lengths only; opening proves the rest.
- Only names of 32 lowercase hex characters with `.mnd`, `.tmp`, `.part` are touched; anything else is left and counted.
- A row with a missing file is counted, not repaired; the page says how many (phone: only rows with the switch on).
- Nothing logs a name or hash.

## 9. Sync

Rows travel as any rows (section 10); bytes travel after them in the same sealed connection (`src-tauri/src/sync/files.rs`). Protocol 2 became 3.

| Direction | Which files |
|---|---|
| Phone to desktop | Every file the desktop lacks and the phone holds (the desktop is the archive) |
| Desktop to phone | Only files with `keep_on_phone` = 1 after the rows merged, that the phone lacks |

- The sender enforces this: a request for a switched-off file, an unknown document, a start beyond the end, or a file that no longer opens gets a `file` message with `skipped` (not `error`, which would end the whole sync).
- A one-way sync carries files only that way; a request against it ends the sync.

**The phase**, after `done`, desktop first; also runs when no table differed.

1. `have`: each side lists `sync_id`s whose complete file it holds (400 per message, last marked).
2. `need`: each side lists `[sync_id, offset]` it wants and the rules allow (phone only switched-on documents). `offset` is the valid prefix of `name.part` (a multiple of 65,536), else 0. Newest first; the desktop's `need` goes first.
3. For each wanted file the sender sends one `file` (`id`, `offset`, `size`, or `skipped`) then `chunk` messages (`data` base64url of up to 24 KiB, `last`). Order is the addressing. Each device reseals under its own key.
4. The receiver seals into `name.part`, hashes as it goes, renames on a SHA-256 match, removes the part and counts a failure on a mismatch. A wrong size or start, bad base64url, a file longer than its row, or a stray message ends the sync as a bad message. A skipped or unwritable file counts as failed and is asked again next sync.
5. `stored`: the desktop names documents it holds whole among those the phone holds with switch off (re-read and checked against the row unless it just arrived). The phone drops a file when its switch is off **and** the desktop named it in a `stored` of this session. A sync only to the desktop sends no `stored`.
6. Summary lines: received, sent, could not be moved, waiting, taken off this phone.

- **Budget.** 100 MiB of file bytes per sync, both ways together (desktop's `need` first, phone gets the rest, each sender stops there and answers `skipped`). A file is asked for only if the rest fits; the next sync resumes from the valid prefix. A session ends at 256 MiB in all.
- **A broken connection** in this phase is not a failed sync: rows are in, the summary notes the break. Rule violations end the sync. A lock stops it with "locked".
- `open/` and temporary files never cross.

## 10. Merge rules for the rows

Newer stamp wins as a whole row; a deletion is a change; derived stamps come from stamps, never the clock.

| Case | Rule |
|---|---|
| Document edited on both (name, date, switch) | Newer whole row wins; a real conflict goes to the review dialog |
| Edited on one, deleted on the other | Newer wins. If the edit wins, the document returns with the links the editor holds. A deleted document removes its file on every device that applies the marker |
| Same link added on both | Matched by `(document, target)`: newer stays (tie: lower id), the other becomes a marker (`loser_marker`) |
| Unlinked on one device | A newer marker; the other deletes its copy |
| Parent deleted on one device while the other attaches a document | The parent's marker takes the link with it. A link row for a parent known as deleted is **not taken and not an error**; the other device drops the link with the parent in the same sync. No link marker on either |
| Unlinked then linked again | Different random ids; the old marker does not touch the new link |
| Document row with other size, kind or hash under a known id | Refused; the sync stops with nothing applied |
| Link row with another document or target under a known id | Refused |
| Two documents with the same hash | Both kept |

- Apply order: settings, categories, recurring items, budgets, transactions, **documents, document links**, then markers (transactions, budgets, recurring items, **links, documents**). An unknown reference that is not a known deletion stops the sync. Files of a deleted document are removed after commit.
- Row checks on arrival: section 6 name rule, closed media type, size 1 to 26,214,400, 64 hex hash, real date, switch 0 or 1, a link with exactly one target. A failure ends the sync naming the table, never the content.
- `MAX_ROWS_PER_TABLE`, the 400-entry digests, table hashes and the conflict limit of 200 apply unchanged.

## 11. Backup, restore and "Delete all my data"

**Backup.** "Include documents" is a choice (the dialog shows the library size; on by default under 100 MB). Scheduled backups keep it in `backup_schedule`; the newest 10 are kept. A backup stays one encrypted file.

Format 1 (one sealed JSON blob in memory) cannot carry gigabytes. Format 2 keeps the header and Argon2id and replaces the seal with a STREAM of 64 KiB chunks as in section 4, holding records (one-byte tag, a length):

```
S  the snapshot (the JSON of today's backup)           once, first
F  a file: sync_id (16) || size (8) || sha256 (32) || the bytes     0 to many
E  the end                                              once, last
```

- Without documents: only `S` and `E`. Version 1 files still open.
- Streamed to a temporary file and renamed; read streamed. The 512 MiB ceiling is per record (snapshot 512 MiB, a file 25 MiB). Truncation is caught by the last-chunk flag.

**Restore:** the snapshot is checked first in a scratch database, then:

1. Each `F` record is hashed as read and sealed under the target vault's `docs_root` into `documents/`.
2. The database is replaced in one transaction.
3. The section 8 check removes files no row names.

A crash before step 2 leaves stray files (removed) and old data; after it, old files with no row (removed). A restore without documents leaves them "missing" until the phone returns files. A restore turns on `applying`, gives a new device id and no pairings.

**As built** (batch 8):

- Header is the old 54 bytes with format 2; its nonce field is random filler (each chunk has its own nonce: last-chunk flag first, chunk number in the last eight bytes); the header is authenticated with every chunk.
- The writer holds one chunk back to learn whether more follows (an unreadable file is left out, so the total is unknown). A wrong passphrase fails the first chunk (`WrongCredentials`); a later failure is `InvalidBackup`. Empty chunks, short non-last chunks and anything after the end record are refused.
- Writing: files are read one at a time under a short vault hold; a lock mid-way leaves nothing (an older file under the name is untouched). A document with no good file is left out and counted. The phone makes no backups. The CLI's `backup.create` still writes rows only.
- Restoring: a file is taken only if a row of the checked snapshot names it with the same size and hash, once; every file is written from its first byte; a failing file is removed at once; a failed new-install restore leaves the app not set up.

**Delete all my data** and the phone's erase remove `documents/` and `open/` in `remove_everything`. Without the key they could not be read; removal is for the disk.

## 12. Export

JSON, CSV and Excel list the documents (name, date, type, size, hash, names of what they link to) and never contain a file; the PDF is unchanged. A limited export lists only documents linked to what it contains. Export functions take only the database, never the folder or keys.

- *JSON:* a `documents` list (`id`, `name`, `date`, `type`, `sizeBytes`, `sha256`, `links`), oldest paper first; each link has `kind` (`transaction` or `recurring`), `id`, `name`, `date`. File `version` stays 1.
- *CSV:* a last column `documents`, names joined by `; ` in name order. Documents attached to nothing or only to a recurring item are not in a CSV.
- *Excel:* a `Documents` sheet (name, date, type, size in bytes, hash, "Attached to": `Name (date)` or `Name (recurring)`) and a `Documents` column on the Transactions sheet. Names go through `neutralise`.
- *Selection:* no limits lists every document. Any limit lists a document only if attached to a transaction in the export or a recurring item that is (category only), and only those attachments, so a name hanging on something left out is never given away.
- The preview counts what the JSON lists; the CLI approval dialog says how many documents are listed and that files are never in it.

## 13. CLI and API

The command API stays on loopback; every route needs a token.

| Route | Token | What |
|---|---|---|
| `documents list`, `documents show` | read | Names, dates, types, sizes, hashes, links. Never contents |
| `documents add <path>` | write | The CLI reads the file itself and sends the bytes in one request body (`data`, base64url). The app never opens a path for a token. This route's body may be 35 MiB; others 1 MiB |
| `documents update` (name, date, phone switch; at least one), `link`, `unlink`, `delete` | write | Same rules as the screens |
| `documents get <id> --out <path>` | write + the person's yes | A dialog (token, document, size, exact file); new owner-only file only; no answer in two minutes is a no |

Eight routes in `src-tauri/src/api/documents.rs`. `show` returns the document with `links`; `add` returns the document and `alreadyThere`.

- **Bigger body:** after the head and before the body, `admit_large` makes the first checks `handle` makes (host, no origin, POST, a route that takes a big body, known token, scope). A would-be-refused caller is refused from the head alone, so 30 MiB is never read without a token. A locked app lets none in.
- **Getting a file out:** a read token is refused before anything is asked. The copy's name must end in the extension of the kind the app found (`.pdf`, `.png`, `.jpg`, `.gif`, `.webp`, `.txt`, `.html`, `.csv`, `.bin` for unknown), never one the caller picks (`start.desktop` or `run.sh` would run something). Created new in one step with the not-there check (`files::write_new`). No file here: `not_found` before asking. A file that no longer opens: `internal` after the yes, nothing written.
- Errors: existing codes only (`invalid_input`, `not_found`, `too_large`, `file_exists`, `forbidden`, `denied`, `locked`, `internal`).
- `cli/schema.json`, `docs/site/examples.json` and `docs/CLI.md` are regenerated with the code.

## 14. The screens

- **Documents page** (sidebar, `g o`; phone: top-bar icon): newest paper first, 50 per page, search by name, type choice (all, images, pdf files, other files), total size, count with no file, "add file" (phone: "take photo"), drag and drop. A card has **details**, **save** (not on a phone) and **delete**.
- **Details:** the picture if drawable, name, date, "keep a copy on my phone", **about this file**, **linked to** (with detach), **open**. The first open of a session explains the plain copy; unknown kinds have no open button.
- **Attach area:** in transaction and recurring dialogs and, short, in the phone's quick add. On an existing item a file is stored and linked at once; on a new one it waits and is sent after saving; a failure still saves the item and names the file.
- **Paper clip** with a number marks items with documents (one `document_counts` call per list page).
- Seven languages; counts as "label: number" (no plural forms). Mobile-first: 44 px targets, 16 px inputs.

**Preview** (2026-10-17). A **preview** button on each card and in details shows an image, PDF, text, CSV, Word or Excel file in a wide dialog.

- **One library, only for PDF:** no single library draws all three without uploading the file, so one new dependency, `pdfjs-dist` (Apache-2.0, `legacy` build), loaded only for PDFs. Word and Excel use about 700 lines of the app's own code in `ui/src/lib/preview/` (`zip.ts`, `xml.ts`, `docx.ts`, `xlsx.ts`, `csv.ts`; zip parts through `DecompressionStream`).
- **Made for hostile files:** the zip reader never writes, finds parts by fixed name; a part stops at 16 MiB when unpacked, 40 MiB per file, 20,000 entries, no zip64, no encryption. The XML reader refuses any document type, reads only five named entities and number references, no namespaces, at most 2 million elements, depth 100. Text is cleaned of control, direction and zero-width characters and drawn as React text.
- **Rust gives bytes and kind:** `read_document_preview` answers `{ kind, mediaType, data }` (base64url). Kinds: `image`, `pdf`, `text`, `csv`, `docx`, `xlsx` (the last two by name plus a zip-header check). HTML never. At most 16 MiB (images 10 MiB, 64 megapixels), else `invalid_input`.
- **Content policy unchanged:** pdf.js runs from the app's files (`script-src 'self'`), no `eval`, `useWasm: false` (no `wasm-unsafe-eval`), bytes and no address. A JPEG 2000 picture in a PDF is not drawn.
- **Look:** PDF one canvas fitted to the dialog, previous/next, wheel turns pages (events under 120 ms apart are one gesture); Word a white sheet; Excel and CSV a sheet with lettered columns, numbered rows and sheet tabs.
- **Links and previews on items:** the number on a card opens `DocumentLinksModal`; a choice navigates with `state: { focus }` (transactions `/app/transactions`, recurring `/app/recurring`, debt entries `/app/debts/<friend>`, so `LinkView` has `personId` for ledger entries only). Rows carry `data-item="kind:id"`; `useJumpTo` scrolls and outlines the row for 2.5 s (gives up after 4 s). The clip (`ClipPreview`) lists an item's documents with a preview each. The recurring pay dialog attaches waiting files and chosen documents to the transaction the payment makes.
- **Limits, said on screen:** Word shows text, headings, bullets and tables, not pictures, text boxes, headers, notes or look. Excel shows last values, dates by format, 1,000 rows and 40 columns, not colours, merged cells, charts or hidden sheets.
- **Not seen yet:** Android System WebView and WebKitGTK with pdf.js. Without `DecompressionStream`, Word and Excel show "could not be read".

## 15. The spike on a real phone

**Done 2026-10-23:** the maintainer checked the finished app on a real phone and found it working. Ticked on their word; the answers to each line were not written down.

Before the screens, one small build on the Redmi Note 11, to settle what the web view can do without a plugin:

1. A file field with `accept="image/*,application/pdf"`: does Android's chooser open, and can the page read the chosen file with `arrayBuffer()`?
2. The same field with `capture="environment"`: does the camera open, does it need the `CAMERA` permission from the app (the barcode scanner plugin may already bring it; not checked), and is the photo readable?
3. Do pieces of 340 KiB of text pass through a command on Android without trouble, and how long does a 25 MiB file take to go in? (Raw bytes are known not to work there, so they are not tried.)
4. Is an image of a few megabytes, given as text and drawn from a `data:` address, shown without delay?
5. Can the system open a PDF from the app's folder (the opener plugin on Android may need a `FileProvider` that the generated project does not have)?

**As of batch 11 the screens and commands that depend on the answers are built**, so the spike is now a check of the finished app on the phone rather than of a sample build. Run it with a debug or release APK of this branch (`scripts/android-patch.py` and the Android build of phase 8 are unchanged) on a phone paired with a desktop, and write down for each line what happened:

1. Documents (top bar, the page icon) → **add file**: does Android's chooser open, and does a chosen PDF show up in the list with the right size and kind? Then a 5 MB photo, and a file over 25 MiB (it should be refused in the page).
2. **Take photo**: does the camera open (watch for a `CAMERA` permission question; the barcode scanner already asked for it for pairing), and is the photo stored and drawn in details?
3. The size of the pieces: add the largest file you have under 25 MiB and note how long "adding" says it takes (the page sends 256 KiB pieces as text). If it is slow, say how slow.
4. **Details** of a photo of a few megabytes: is it shown without a long wait? (It is drawn from a `data:` address.)
5. **Open** a PDF that is kept on the phone: does the system offer a viewer? (The opener plugin may need a file provider that the generated project does not have.) If it fails, the page shows the error and the plan is "open this on your computer" for everything but images.
6. A sync with **keep on phone** on for one document and off for another: the first arrives, the second stays "on the computer"; add a file on the phone with the switch off and sync: it should reach the desktop and leave the phone.
7. Lock the phone's app with a file open in a viewer, and check that `open/` is empty afterwards (it cannot be looked at without a debug build; the page shows no sign of a leftover).

The answers go into this file. If 1 or 2 fail, a small plugin or a patch in `scripts/android-patch.py` takes their place, and that is a dependency decision to make with the maintainer. If 5 fails, the phone shows images in the app and says "open this on your computer" for the rest, which is acceptable for a first version.

## 16. Open points

- The real-phone check (section 15).
- ~~Backup dialog default.~~ On under 100 MB, off above; changeable.
- Thumbnails: none; a later one would be a second sealed file per image.
- Windows: removal of an open copy while a viewer holds it is described, not seen.
- ~~Phone documents page location.~~ Top-bar icon; to be looked at on the real phone.
- A total size limit or warning for the phone.
- A review by someone else of the sealed format and file transfer before release.

## 17. Tests

- **Sealed format:** known answers from `scripts/doc-vectors.py` (a second implementation) for keys, names and small files; a changed byte in the header or any chunk, cuts at and inside every chunk, an appended or swapped chunk, another file's chunk, another document's name: all refused; the largest file; round trips for sizes 0 to 3 chunks; the key dropped on lock.
- **Names and types:** hostile names (paths, control and direction characters, long, reserved Windows names, only dots); media type against lying extensions; a huge-header image is not drawn.
- **Rows:** constraints, triggers (stamps, markers, audit), deleting a transaction removes links not documents, deleting a document removes links and file.
- **Agreement:** a file without a row removed, a row without a file counted, `.tmp` and old `.part` cleaned.
- **Sync:** every section 10 case; random document and link edits on two devices converge; a switched-off document never crosses (a marker searched in everything that crossed); the phone's new document leaves the phone only after confirmation; a transfer stopped at every chunk resumes; tampered, mis-hashed, out-of-order or unasked chunks are refused; the byte budget leaves the rest waiting.
- **Backup:** with and without documents; restore into another key; cut or changed backups refused leaving the app as it was; version 1 opens; a simulated crash is repaired.
- **Wipe and lock:** `open/` empty after a lock; neither folder after "delete all my data".
- **API:** a read token sees no contents; an over-25 MiB upload is refused before it is read; `get` writes only after Allow and never over a file.
- **Screens:** looked at in a browser at desktop and phone width with commands faked; the i18n key test.
- **By hand:** the real phone, with a photo and a PDF (section 15), then the plan's "done when".
