# Documents: a guide

Documents keep the paper trail next to the numbers: a receipt, an invoice, a contract, a warranty. A document is a file that meowNY stores **encrypted** on your computer (and, if you choose, on your phone), with a name, a date and the transactions and recurring items it belongs to. Nothing is uploaded anywhere, and no file is ever read by a program that is not on your own computer.

This is the guide for using them. How they are built is in [documents-design.md](documents-design.md), and exactly what is stored where is in [data-inventory.md](data-inventory.md).

## Adding a document

**On the computer.** Open **documents** in the sidebar and press `n`, or the **add file** button, and choose one or more files. You can also drag files from your file manager onto the window; each becomes a document named after the file, dated today. To attach a file to something as you make it, open the transaction (or recurring item) and use the **attach** area: **add file** stores a new one and **attach** searches the documents you already have.

**On the phone.** Open **documents** from the bar at the top. **Add file** opens Android's own chooser, and **take photo** opens the camera. The round "+" opens the normal add-transaction form, which has the same attach area (with the photo button), because a receipt is most often photographed at the till.

What the app does with a file:

- It keeps it **encrypted**, under a key made from the key of your database. Locking the app drops the key, so a locked app cannot read a single file.
- A file may be up to **25 MiB**. The date is today unless you give one (it is the day of the paper, not the day you added it).
- If you add exactly the same file again, nothing is stored twice: the app says it is already in the library and attaches it to what you asked.
- The **name** is cleaned: only the file's own name (never a path), no control or invisible characters, one plain space between words, at most 120 characters. You can rename it later.
- The app works out the **kind** of file from its first bytes, not from its name: a PDF, an image (JPEG, PNG, GIF, WebP), plain text, CSV, HTML, or "other". A file that is called `.jpg` and is not a picture is never drawn.

## Attaching, opening, saving

- On the documents page a card's second line says the date, the size and, after the size, a paper clip with the number of things the document is attached to.
- A paper clip with a number on a transaction, a recurring item or a debt entry says how many documents are attached. On the transactions list and on the recurring list it is on the first line, after the date and after how often it repeats. **Press it** to get the list of that item's documents: each has its own **preview** button, and its name opens its details. (To change what is attached, use **edit** on the item.)
- On the documents page the number on a card is a button. It opens a list of everything that document is attached to (transactions, recurring items, debt entries); **choose one to go to it**: the page opens (a transaction on its month), scrolls to the item and outlines it for a moment.
- In an item's own dialog (**edit**), each document attached to it has a **preview** button too.
- **Mark as paid** on a recurring item has a **documents** area under the amount and the date: add a file, or attach one from the library. It is attached to the **transaction the payment makes** (not to the recurring item), so a receipt is kept with that payment.
- **Details** on the documents page opens a dialog with the picture (for images the app may draw), the name, the date (the day on the paper), the box **keep a copy on my phone**, a section **about this file** (its kind and size, and where it is if it is not here), **linked to** (what it is attached to, with **detach**) and **open** (which says it shows the file in your system's viewer). **Preview**, **save** (to keep a copy where you choose) and **delete** are buttons on the card in the list, beside **details**; **preview** is in the dialog too.
- **Open** hands the file to your system's own viewer, from a **plain copy** made for the occasion. The first time in a session the app says what that means and asks. The copy is not encrypted while it is open; it is removed when you lock the app, when you close it, when the app starts again and on "delete all my data". On Windows a viewer that is still holding the file can stop its removal until the next start, so close the viewer first. An HTML file may load things from the internet when the system opens it.
- Files of the kind **other** (a program, an archive, anything the app cannot recognise) are never opened from the app. They can only be **saved as a copy**, and what happens to them then is up to you. This is on purpose: your system chooses what to start by the file's extension, and the extension is the one thing whoever sent the file chose.
- **Save a copy** (on the computer) asks where, never replaces a file that is there, and the copy is not encrypted.
- **Delete** asks first and names the file. It removes the document, its file and its attachments; the transactions stay.

## Previewing a document

**Preview** (on each card, before **details**, and in the details dialog) shows what is in the file, inside the app. It makes no plain copy and starts no other program.

- **Images** are drawn. A **PDF** is drawn one page at a time, as large as the dialog lets it be, with **previous**, the page number and **next** under it; the mouse wheel (or a flick on a trackpad) turns the page too. **Text** is shown as text.
- A **Word** file (.docx) is shown as a sheet of white paper with its text, headings, lists and tables. An **Excel** file (.xlsx) and a **CSV** are shown as a sheet: letters over the columns, numbers beside the rows, a line round every cell and numbers to the right; an Excel file has the tabs of its sheets under the grid, shows the first 1,000 rows and 40 columns of each, and dates as dates. The dialog says what is not shown: pictures, text boxes, charts, formulas, colours and the look of the page. To see those, **save** a copy and open it in your own program.
- The old Word and Excel formats (.doc, .xls), PowerPoint, HTML (the app would have to draw someone else's page) and files of no known kind are not previewed, and the button is dimmed. So is a file bigger than 16 MiB (10 MiB for an image), and a file this device does not hold.
- Word and Excel files are told by their **name** (.docx, .xlsx, and their macro forms .docm, .xlsm), because their first bytes say only "zip"; the file must also really be a zip. A file renamed to have no extension is not previewed until it has one again.
- The file is only **read**: no macro, script or link in it is run or followed, and nothing is fetched. It is not written to disk; the page holds it in memory while the dialog is open. Characters that hide or reverse text are taken out of what is shown.
- A PDF that does not carry its fonts is drawn in your system's fonts, so it can look a little different. A picture coded as JPEG 2000 inside a PDF (rare) is not drawn.

## Keeping a document on your phone

Every document is kept on the computer, which is the archive. A document goes to the phone only if you tick **keep a copy on my phone** for it, and then at the next sync. A document you did not switch on never leaves the computer, even if the phone asks for it.

- A document **added on the phone** goes to the computer at the next sync. If its switch is off, the phone then lets go of its own copy, but **only after the computer has said, in the same sync, that it holds a whole and checked copy**. Until then it stays on the phone.
- The files move after the rows, in the same encrypted connection as the rest of the sync. A sync that is cut half way carries on from where it stopped at the next one, and a file that arrives damaged is refused and asked for again. At most about 100 MB of files move in one sync; the rest waits and the sync says how many are waiting.
- On the phone a document that is not kept there shows "on the computer". The documents page says how many documents are on the device and how big they are.
- If you rename the same document on both devices before they meet, the device that takes the changes in shows both versions side by side, like any other conflict, and you choose.

## Backups, restore and export

- **A backup** (Settings, backup) has an **include the documents** box. It shows how many files and how much room they take, and starts on while that is under 100 MB. The backup is still one encrypted file, written as it is read, so a big library is fine. A scheduled backup remembers the choice and says how much room the newest ten backups can take. A document whose file is not on this computer is left out and counted.
- **Restoring** brings back the rows and the files in the backup. A backup made without the documents restores the rows and leaves the files missing; the documents page says so, and the phone fills them in at the next sync if it still holds them. A restore that fails or is cut anywhere changes nothing.
- **Export** (JSON, CSV, Excel) lists the documents by name, date, type, size, hash and what they are attached to, and **never contains a file**. A limited export lists only the documents attached to what it contains. The files go in a backup, not in an export, and an export is not encrypted.

## From a script

The command line has the same things (see the [CLI reference](CLI.md)):

```bash
meowny documents add ~/Downloads/boiler-receipt.pdf --date 2026-10-01 --transaction-ids 42
meowny documents list --q boiler
meowny documents show 7
meowny documents link 7 --recurring-id 3
meowny documents get 7 --out ~/boiler-receipt.pdf      # you are asked in the app
meowny documents delete 7 --yes
```

A read-only token can list and show documents (names, dates, types, sizes and hashes), never a file. A token that can write can add, rename, attach and delete. **Getting a file out always needs your yes in the app**, and the copy must have the extension of its kind (`.pdf`, `.png`, ...), so a script cannot make a file a system would run.

## What to know

- **A locked app shows nothing.** The files are sealed with a key that exists only while the app is unlocked.
- **A file's name on disk says nothing about it.** The folder `documents/` next to your database holds files with random-looking names, so it shows how many files there are and about how big each is, and nothing else. Do not edit it by hand: a file that no document names is removed the next time the app is unlocked, and a document whose file is gone is marked missing.
- **A plain copy exists while a file is open in another program.** That is the one time a file is readable outside the app. It is in the folder `open/`, removed on lock and exit.
- **No thumbnails.** A list that decrypted and drew every picture would be slow, so images are drawn only in the details.
- **Not seen yet on a real phone:** the camera and the file chooser of the Android app, opening a PDF from the phone, and drawing a large photo. They are the checklist in [documents-design.md](documents-design.md), section 15.
