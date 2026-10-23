# Documents: a guide

A receipt, invoice, contract or warranty, stored **encrypted** on your computer (and on your phone if you choose), with a name, a date and the transactions and recurring items it belongs to. Nothing is uploaded.

Built: [documents-design.md](documents-design.md). Stored where: [data-inventory.md](data-inventory.md).

## Adding a document

- **Computer:** **documents** in the sidebar, then `n` or **add file**. Or drag files onto the window (named after the file, dated today). In a transaction or recurring item, the **attach** area: **add file** stores a new one, **attach** finds one you have.
- **Phone:** **documents** in the top bar. **Add file** opens Android's chooser, **take photo** the camera. The round "+" form has the same attach area.

What the app does:

- Keeps the file encrypted with a key from your database key. A locked app cannot read any file.
- Up to **25 MiB**. The date is today unless you give one (the day of the paper).
- The same file added again is not stored twice; it is attached to what you asked.
- The name is cleaned: file name only, no control or invisible characters, at most 120 characters. You can rename it.
- The kind comes from the first bytes, not the name: PDF, image (JPEG, PNG, GIF, WebP), text, CSV, HTML, or "other".

## Attaching, opening, saving

- A paper clip with a number on a transaction, recurring item or debt entry counts its documents. **Press it** for the list; each has **preview**, its name opens details.
- On the documents page, the number on a card lists everything the document is attached to; choose one to jump to it.
- **Mark as paid** on a recurring item has a **documents** area. The file is attached to the transaction the payment makes.
- **Details** shows the picture (images), name, date, **keep a copy on my phone**, **about this file**, **linked to** (with **detach**) and **open**. **Preview**, **save** and **delete** are buttons on the card.
- **Open** hands a plain copy to your system's viewer; the first time per session the app says so and asks.
  - The copy is not encrypted while open. It is removed on lock, close, next start and "delete all my data".
  - On Windows a viewer still holding the file can delay removal until the next start. Close the viewer first.
  - An HTML file may load things from the internet when the system opens it.
- Kind **other** (programs, archives, anything unknown) is never opened from the app, only **saved as a copy**. Your system picks what to start by extension, and the sender chose the extension.
- **Save a copy** (computer) asks where, never replaces a file, and the copy is not encrypted.
- **Delete** asks first. It removes the document, its file and its attachments; transactions stay.

## Previewing

**Preview** shows the file inside the app: no plain copy, no other program.

| Kind | Shown as |
|---|---|
| Image, text | drawn / shown |
| PDF | one page at a time, **previous** / **next**, wheel turns pages |
| Word (.docx) | white paper with text, headings, lists, tables |
| Excel (.xlsx), CSV | a sheet; Excel shows sheet tabs, first 1,000 rows and 40 columns, dates as dates |

- Not shown for Word/Excel: pictures, text boxes, charts, formulas, colours, page look. **Save** a copy to see them.
- Not previewed (button dimmed): .doc, .xls, PowerPoint, HTML, unknown kinds, files over 16 MiB (10 MiB for images), files this device does not hold.
- Word and Excel are told by name (.docx, .xlsx, .docm, .xlsm) and must really be a zip.
- The file is only read: no macro, script or link runs. It is held in memory, not written to disk. Characters that hide or reverse text are removed.
- A PDF without its fonts uses your system's fonts. A JPEG 2000 picture inside a PDF is not drawn.

## Keeping a document on your phone

- The computer is the archive. A document goes to the phone only if **keep a copy on my phone** is ticked, at the next sync. Others never leave the computer, even if the phone asks.
- A document **added on the phone** goes to the computer at the next sync. If its switch is off, the phone then drops its copy, but only after the computer confirms in the same sync that it holds a checked copy.
- Files move after the rows, over the same encrypted connection. A cut sync resumes; a damaged file is refused and asked again. About 100 MB per sync; the rest waits and the sync says how many.
- On the phone, a document not kept shows "on the computer". The page shows how many documents are on the device and their size.
- A rename on both devices shows both versions side by side to choose from.

## Backups, restore and export

- **Backup** (Settings, backup): **include the documents** box shows file count and size; on by default under 100 MB. Still one encrypted file. A schedule remembers the choice and shows what the newest ten can take. A document with no file here is left out and counted.
- **Restore** brings back rows and files. A backup without documents restores the rows with files missing (the phone fills them at the next sync). A failed restore changes nothing.
- **Export** (JSON, CSV, Excel) lists documents by name, date, type, size, hash and attachments, and **never contains a file**. A limited export lists only documents attached to what it holds. Exports are not encrypted.

## From a script

```bash
meowny documents add ~/Downloads/boiler-receipt.pdf --date 2026-10-01 --transaction-ids 42
meowny documents list --q boiler
meowny documents show 7
meowny documents link 7 --recurring-id 3
meowny documents get 7 --out ~/boiler-receipt.pdf      # you are asked in the app
meowny documents delete 7 --yes
```

- A read-only token lists and shows (names, dates, types, sizes, hashes), never a file.
- A write token can add, rename, attach and delete.
- Getting a file out always needs your yes in the app, and the copy must have the extension of its kind. See the [CLI reference](CLI.md).

## What to know

- A locked app shows nothing.
- The `documents/` folder holds random-looking names: it shows how many files and roughly how big, nothing else. Do not edit it: a file no document names is removed at the next unlock; a document with no file is marked missing.
- A plain copy exists only while a file is open in another program, in `open/`.
- No thumbnails: images are drawn in details only.
- The phone side was checked on a real phone by the maintainer on 2026-10-23. Checklist: [documents-design.md](documents-design.md), section 15.
