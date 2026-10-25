// The guide: what each part of meowNY does, and what it keeps where, in a few lines each. This page is the user
// documentation; there is no Markdown copy of it. How sync, documents and debts are built is in the design notes in docs/.

import { REPO, SPONSOR } from './render.mjs'
import { gallery } from './shots.mjs'

const doc = (name) => `${REPO}/blob/main/docs/${name}`
const RELEASES = `${REPO}/releases/latest`

/** The screenshots each section shows, under its text; missing files are left out. */
const PICTURES = {
  start: ['lock', 'phone-lock'],
  budgeting: ['dashboard', 'categories', 'recurring', 'transactions'],
  documents: ['documents', 'preview-pdf', 'preview-csv'],
  debts: ['debts', 'friend', 'split', 'bill-code', 'debt-history'],
  phone: ['phone-sync', 'phone-dashboard', 'phone-quick-add', 'phone-debts', 'phone-categories'],
  backup: ['export'],
  privacy: ['settings'],
}

const SECTIONS = [
  {
    id: 'start',
    title: 'Getting started',
    blocks: [
      { t: 'p', text: 'A budget app for your computer (Linux, Windows) and your Android phone. Your data stays on your devices, encrypted.' },
      { t: 'note', kind: 'info', text: '**Pre-release.** Back up before you update. Nobody outside the project has reviewed the sync protocol yet.' },
      { t: 'h3', id: 'install', text: 'Install' },
      { t: 'p', text: `Every download is on [the latest release](${RELEASES}). Pick your system below. Updating keeps your data.` },
      { t: 'p', text: '**Linux** (Ubuntu, Debian and the like)' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_amd64.deb`.',
          'Install it: `sudo apt install ./meowNY_<version>_amd64.deb`. This also adds the `meowny` command line.',
          'Open **meowNY** from your app menu.',
        ],
      },
      { t: 'p', text: 'Other Linux: download the `.AppImage`, make it runnable with `chmod +x meowNY_<version>_amd64.AppImage`, then run it. **To update**, install or run the new file the same way.' },
      { t: 'p', text: '**Windows**' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_x64-setup.exe`.',
          'Run it. The installer is not signed yet, so Windows warns you: choose **More info**, then **Run anyway**.',
          'Open **meowNY** from the Start menu.',
        ],
      },
      { t: 'p', text: '**To update**, run the new installer.' },
      { t: 'p', text: '**Android**' },
      {
        t: 'ol',
        items: [
          'On the phone, download `meowNY_<version>_android.apk`.',
          'Open it. If Android asks, allow your browser or file manager to install apps.',
          'Open **meowNY**, then pair it with your computer: [your phone](#phone).',
        ],
      },
      { t: 'p', text: '**To update**, open the new APK, or add this repository to [Obtainium](https://github.com/ImranR98/Obtainium) to get each new version.' },
      { t: 'note', kind: 'warn', text: `**iPhone and Mac: not available yet.** There is no way to install meowNY on an iPhone or a Mac today. Publishing on the App Store and Google Play needs paid developer accounts. To help make that happen, [support meowNY on Ko-fi](${SPONSOR}).` },
      { t: 'p', text: 'Check a download with `sha256sum -c SHA256SUMS --ignore-missing`. To build meowNY yourself: [develop](develop.html).' },
      { t: 'h3', id: 'first-run', text: 'First run' },
      { t: 'ol', items: ['Pick a currency and the name friends see on a bill code.', 'Add a passphrase, if you want one.', '**Write down the recovery key.** It is shown once, and it is the only way back in after a forgotten passphrase.'] },
      { t: 'h3', id: 'data', text: 'Where your data lives' },
      {
        t: 'table',
        head: ['System', 'Folder'],
        rows: [
          ['Linux', '`~/.local/share/org.meowny.app/`'],
          ['macOS', '`~/Library/Application Support/org.meowny.app/`'],
          ['Windows', '`%APPDATA%\\org.meowny.app\\`'],
        ],
      },
      { t: 'p', text: 'The key is in your system keychain (GNOME Keyring or KWallet, Windows Credential Manager, macOS Keychain). Updating the app never touches this folder.' },
    ],
  },
  {
    id: 'budgeting',
    title: 'Budgeting',
    blocks: [
      {
        t: 'ul',
        items: [
          '**Categories** sort transactions into income and expenses. Hold one and drag it to change the order (or Alt + arrow keys).',
          '**Budgets** per month and category. A budget carries into later months. Change a past month and choose: only that month, that month and later, or every month.',
          '**Recurring items:** pay, skip, pause or end them. One with no fixed amount asks for it when paid.',
          '**Statistics:** budget against spending, trends, a year view.',
          'Seven languages, light and dark themes.',
        ],
      },
    ],
  },
  {
    id: 'documents',
    title: 'Documents',
    blocks: [
      {
        t: 'ul',
        items: [
          'Attach receipts and invoices to transactions, recurring items and debt entries: **add file**, drag files onto the window, or **take photo** on the phone. The paper clip on a row counts them.',
          'Each file is encrypted, up to 25 MiB. The same file added twice is stored once. Its kind is read from its first bytes, not its name.',
          '**Preview** shows images, PDF, text, CSV, Word (.docx) and Excel (.xlsx) inside the app. Nothing in the file runs, and nothing is written to disk.',
          '**Open** hands a plain copy to your system viewer; it is removed on lock, close and next start. Unknown kinds can only be **saved as a copy**.',
          '**Keep a copy on my phone** picks which documents go to the phone. The computer is the archive; a document added on the phone moves to the computer at the next sync.',
          'Backups can include the files. Exports list documents by name, never the files.',
        ],
      },
      { t: 'note', kind: 'info', text: 'Do not edit the `documents/` folder by hand: a file no document names is removed at the next unlock.' },
    ],
  },
  {
    id: 'debts',
    title: 'Debts and shared bills',
    blocks: [
      {
        t: 'ul',
        items: [
          'A friend is just a name: no phone, email or contacts.',
          '**Lend, borrow, paid back:** a ledger per friend, outside your budget. Positive means they owe you. **Mark as paid** settles all or part.',
          '**Split a bill** equally, by percent, shares or amounts. Budgets, statistics and exports count only your part; each friend\'s part is added to what they owe.',
          '**Bill codes:** **show code** makes a QR code with one friend\'s part (and nobody else\'s). They **scan a bill** or **paste a code**, check it, and accept; nothing is written before that.',
          '**Due payments** lists recurring payments past their day; **history of debts** shows every line with the balance after it.',
          '**Reminders:** every week, 2 weeks or month after a debt last changed, set per device. The text holds a count, never a name or amount.',
          '**Save statement** writes one friend\'s lines as TXT, CSV or PDF, not encrypted.',
        ],
      },
    ],
  },
  {
    id: 'phone',
    title: 'Your phone',
    blocks: [
      { t: 'p', text: 'The phone app (Android; iPhone is not available yet) keeps a full copy and works offline. It syncs over your home wifi, with no server.' },
      { t: 'h3', id: 'pairing', text: 'Pair once' },
      { t: 'ol', items: ['Computer: **settings › phone › add a phone**.', 'Phone: scan the QR code.', 'Type the six digits the computer shows.'] },
      { t: 'h3', id: 'syncing', text: 'Sync' },
      {
        t: 'ul',
        items: [
          'Computer: **settings › phone › sync with phone**, then scan.',
          'Choose a direction: both ways, to the computer or to the phone.',
          'Conflicts are shown side by side; you pick.',
          'Both devices must be on the same network. A network that keeps devices apart blocks it: use the phone\'s hotspot.',
          'iPhone: allow **local network** when asked, then scan again. Documents are previewed in the app; iPhone has no "open".',
          'A passphrase locks the phone. Five wrong tries erase it; pair again to get the data back.',
          'A lost phone is removed on the computer: **settings › phone**.',
        ],
      },
    ],
  },
  {
    id: 'backup',
    title: 'Backup and export',
    blocks: [
      {
        t: 'ul',
        items: [
          '**Backup:** one encrypted `.meowbak` file under a passphrase you choose, now or on a schedule (the newest 10 are kept). Can include the documents.',
          '**Restore** from settings or the first-run screen. A failed restore changes nothing.',
          '**Moving to a new computer:** back up, install there, restore on the first-run screen, then pair the phone again.',
          '**Export** to JSON, CSV, Excel or PDF, limited by dates, categories and more if you like. Exports are **not encrypted**.',
          '**Delete all my data** wipes the database, documents, keys and settings on this device.',
        ],
      },
    ],
  },
  {
    id: 'privacy',
    title: 'Privacy and security',
    blocks: [
      {
        t: 'ul',
        items: [
          'No server, no account, no telemetry. Nobody but you holds your data.',
          'The database is encrypted (SQLCipher). Its key is kept only as two wrapped copies: one opened by the keychain (and your passphrase), one by the recovery key.',
          'The app locks when idle and forgets the key. A locked app shows nothing, also to scripts.',
          'Logs and crash reports never hold amounts, descriptions, tokens or keys.',
          'Android: no system backup of the app\'s folder, no screenshots. iPhone: kept out of iCloud backups.',
        ],
      },
      { t: 'h3', id: 'stored', text: 'What is stored' },
      {
        t: 'table',
        head: ['What', 'Where', 'Protection'],
        rows: [
          ['Categories, budgets, transactions, recurring items, debts, the change history', '`meowny.db`', 'Encrypted'],
          ['Documents', '`documents/`, one sealed file each with a random name', 'Encrypted, per-file key'],
          ['Lock settings, the wrapped keys', '`vault.json`', 'No financial data'],
          ['Language, sidebar, reminder times, the name on bill codes', 'The window\'s local storage', 'No financial data, not encrypted'],
          ['A document open in another program', '`open/`, only while open', '**Not encrypted**, removed on lock and close'],
          ['Command line tokens', '`meowny.db` (only a hash); the token file of `meowny login`', 'Read-only by default, expire, revocable'],
        ],
      },
      { t: 'h3', id: 'leaves', text: 'What leaves the device' },
      {
        t: 'table',
        head: ['What', 'When'],
        rows: [
          ['Phone sync', 'When you start it. Straight to the paired phone on your home network, sealed with keys from the pairing. No relay or server'],
          ['Files: backups, exports, statements, copies of documents', 'Where you save them. Only backups are encrypted'],
          ['A bill code', 'When you show or copy it: one friend\'s part of one bill, not encrypted'],
          ['Notifications', 'Due items by name on the computer; debt reminders by count only'],
          ['The clipboard', 'The recovery key or a new token, only when you press copy'],
        ],
      },
      { t: 'p', text: 'There are no other network calls. CI lists every place in the code that could open a connection and fails on a new one. The command line API listens on `127.0.0.1` only, is off until you turn it on, and needs a token; exports, backups and copies of documents through it need your yes in the app each time.' },
      { t: 'p', text: '**Delete all my data** removes the database, `documents/`, `open/`, `vault.json` and the keychain entry. It leaves backups and exports you saved elsewhere, and the token file of `meowny login` (which stops working).' },
      { t: 'p', text: `How it is built: [sync](${doc('sync-design.md')}), [documents](${doc('documents-design.md')}), [debts](${doc('debts-design.md')}).` },
    ],
  },
  {
    id: 'shortcuts',
    title: 'Keyboard shortcuts',
    blocks: [
      { t: 'p', text: 'Press `?` in the app to see them all.' },
      {
        t: 'table',
        head: ['Keys', 'Does'],
        rows: [
          ['`g` then `d` `t` `c` `r`', 'dashboard, transactions, categories, recurring'],
          ['`g` then `f` `o` `e` `s`', 'debts, documents, export, settings'],
          ['`n`', 'new'],
          ['`/`', 'search'],
          ['`[` `]`', 'previous, next month'],
          ['`t`', 'this month'],
          ['`j` `k`', 'next, previous row'],
          ['`Ctrl/Cmd+L`', 'lock'],
          ['`Ctrl/Cmd+B`', 'sidebar'],
        ],
      },
    ],
  },
  {
    id: 'cli',
    title: 'Command line',
    blocks: [
      { t: 'p', text: '`meowny` lets scripts and AI agents use the running desktop app. The Linux `.deb` installs it; the release also has it on its own for Linux and Windows.' },
      { t: 'ol', items: ['In the app: **settings › command line access**. Turn it on and make a token.', 'Give it the token: `echo "$TOKEN" | meowny login`', 'Try it: `meowny status`'] },
      { t: 'p', text: 'Full reference: [command line](cli.html).' },
    ],
  },
]

/** The guide; `images` holds the screenshots whose files exist (build.mjs). */
export function buildGuide(images = new Set()) {
  const sections = SECTIONS.map((section) => {
    const html = gallery(PICTURES[section.id] ?? [], images)
    return html ? { ...section, blocks: [...section.blocks, { t: 'figures', html }] } : section
  })
  return {
    file: 'guide.html',
    title: 'meowNY guide',
    description: 'How meowNY works: install, budgets, documents, debts, the phone, backups, privacy and the command line.',
    lead: 'What each part of the app does, and what it keeps where.',
    footer: `meowNY is free software (AGPL-3.0). [Source](${REPO}) · [Support on Ko-fi](${SPONSOR}): it goes toward the App Store and Google Play.`,
    nav: [{ title: 'Guide', links: SECTIONS.map((s) => ({ id: s.id, text: s.title })) }],
    sections,
  }
}
