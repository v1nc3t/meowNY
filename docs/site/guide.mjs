// The guide: what each part of meowNY does, a step more detailed than the landing page. The Markdown guides in docs/
// hold every detail; each section here links to its own.

import { REPO } from './render.mjs'

const doc = (name) => `${REPO}/blob/main/docs/${name}`

const SECTIONS = [
  {
    id: 'start',
    title: 'Getting started',
    blocks: [
      { t: 'p', text: 'meowNY is a budget app for one person on a desktop computer (Linux, macOS or Windows), with an optional Android phone that keeps a full copy. Everything is stored on your own devices, encrypted.' },
      { t: 'note', kind: 'info', text: '**Pre-release (0.1.0).** There are no installers yet, so you build the app from source. Signed installers come with the first release.' },
      { t: 'h3', id: 'install', text: 'Build and run' },
      {
        t: 'ul',
        items: [
          'Node.js 22 or newer, and npm.',
          'Rust 1.90 or newer (from [rustup.rs](https://rustup.rs)).',
          "Tauri's system libraries: see [the prerequisites](https://v2.tauri.app/start/prerequisites/).",
          'A keychain for the key: macOS Keychain, Windows Credential Manager, or GNOME Keyring or KWallet on Linux.',
        ],
      },
      { t: 'code', text: 'git clone https://github.com/v1nc3t/meowNY.git\ncd meowNY\nnpm install\nnpm run tauri dev' },
      { t: 'p', text: 'The first run compiles the Rust side, which takes a few minutes. `npm run tauri build` makes an app and installer for your system (not signed yet).' },
      { t: 'h3', id: 'first-run', text: 'First run' },
      {
        t: 'ol',
        items: [
          'Choose your currency. It can be changed later in settings, with or without converting the amounts.',
          'Choose a passphrase if you want one. Without it, the key in your keychain opens the data on its own.',
          '**Write down the recovery key.** It is shown once, and it opens your data if the keychain or the passphrase is lost.',
        ],
      },
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
      { t: 'p', text: '`meowny.db` is the encrypted database, `documents/` holds the encrypted files, and the key is in your keychain under `org.meowny.app`.' },
    ],
  },
  {
    id: 'budgeting',
    title: 'Budgeting',
    blocks: [
      { t: 'p', text: 'What came in, what went out, and how that compares with your plan.' },
      {
        t: 'ul',
        items: [
          '**Categories** group your transactions and decide whether each one is income or an expense.',
          '**Transactions** can be searched and filtered by date range, category, and income or expense.',
          '**Budgets** are set for each month: one for income, one for expenses, and one for each category if you like. A new amount applies from that month on, so past months keep theirs.',
          '**Recurring items** (rent, salary, subscriptions) show what is due. You pay, skip, pause or end them. A bill whose amount changes asks for the amount when you pay it. Notifications are optional.',
          '**Statistics** show budget against spending, trends, a year view and your top categories.',
          '**Money** is kept in whole cents, never as a floating-point number, and dates follow your time zone.',
          '**Seven languages** (English, German, Spanish, French, Italian, Portuguese and Romanian), and light and dark themes.',
        ],
      },
    ],
  },
  {
    id: 'documents',
    title: 'Documents',
    blocks: [
      { t: 'p', text: 'Receipts, invoices, contracts and warranties, attached to transactions, recurring items and debt entries. They are stored encrypted next to the database. Nothing is uploaded.' },
      {
        t: 'ul',
        items: [
          "**Add** a file on the documents page, drop it on the window, or use a transaction's attach area. On the phone, pick a file or take a photo.",
          'Up to **25 MiB** per file. The same file added twice is stored once.',
          "The kind of file is read from its first bytes, never from its name. Unknown kinds are only saved, never opened.",
          '**Preview** inside the app: images, PDF, text, CSV, Word (.docx) and Excel (.xlsx). Nothing in a file runs, and no plain copy is written.',
          "**Open** hands a plain copy to your system's viewer. It is deleted when you lock or close the app.",
          '**Keep a copy on my phone** is a switch on each document. Documents without it never leave the computer.',
          'Backups can include the files. Exports list documents by name and never contain a file.',
        ],
      },
      { t: 'p', text: `All the details: [the documents guide](${doc('documents.md')}).` },
    ],
  },
  {
    id: 'debts',
    title: 'Debts and shared bills',
    blocks: [
      { t: 'p', text: 'Who owes whom, and the bills you share, in your own ledger. Your friends do not need the app, and nothing reaches them unless you show it.' },
      {
        t: 'ul',
        items: [
          '**A friend is a name** and nothing else: no phone number, email or contacts.',
          '**Lend, borrow, paid back:** entries in a ledger for each friend. They are never income or expenses and never touch a budget.',
          '**Split a bill** equally, by percent, by shares or by exact amounts. The parts add up to the cent. Budgets, statistics and exports count only your part.',
          '**Split recurring items** too. Each payment keeps the same proportions.',
          '**Due payments** lists recurring payments past their day. **History of debts** shows every line, newest first.',
          "**Bill codes:** a QR code holding one friend's part, the bill and your name. The friend scans or pastes it in their own meowNY, then accepts or denies. Nothing is sent anywhere.",
          '**Reminders** on each device: every week, every 2 weeks, every month or never. The notification holds a count, never a name or an amount.',
          "**Statements** for a friend, saved as text, CSV or PDF.",
        ],
      },
      { t: 'p', text: `All the details: [the debts guide](${doc('debts.md')}).` },
    ],
  },
  {
    id: 'phone',
    title: 'Your phone',
    blocks: [
      { t: 'p', text: 'The Android app keeps a full copy of your data, works offline, and can change everything the computer can. Back home, the two sync over your wifi. There is no server or cloud in between.' },
      { t: 'h3', id: 'pairing', text: 'Pair once' },
      {
        t: 'ol',
        items: [
          'On the computer: **settings › phone › add a phone**. A QR code shows.',
          'On the phone: scan it.',
          'Type the six digits the computer shows into the phone. The phone then gets a full copy.',
        ],
      },
      { t: 'h3', id: 'syncing', text: 'Sync' },
      {
        t: 'ul',
        items: [
          'Every sync starts on the unlocked computer: **settings › phone › sync with phone**, then scan the new code. It works once, for about two minutes, and the phone connects straight to the computer.',
          'Choose the direction: both ways, to the computer, or to the phone.',
          'Something changed on both devices is shown side by side, and you choose which to keep.',
          'Everything sent is encrypted with keys from the pairing and that QR code. An old photo of a code, or a stolen phone alone, cannot sync.',
          "Both devices must be on the same network. Networks that keep devices apart (hotels, campuses) do not work; the phone's hotspot does.",
          'Windows or macOS may ask once whether meowNY may accept connections. Allow private networks only.',
        ],
      },
      { t: 'h3', id: 'phone-lock', text: 'The phone is locked' },
      {
        t: 'ul',
        items: [
          'A passphrase opens the phone app. After 5 wrong tries the phone erases its copy; pair again to get it back.',
          "The phone has no export or backup file, and Android's cloud backup is off for the app. The currency is changed on the computer only.",
        ],
      },
      { t: 'h3', id: 'get-apk', text: 'Get the app' },
      { t: 'p', text: `For now, the Android app is a debug build that you install by hand. GitHub Actions builds it on every change (the **Android** workflow, artifact \`meowny-android-debug\`). To build it yourself, see [the development notes](${doc('development.md')}#android). The sync, step by step: [the sync design](${doc('sync-design.md')}).` },
    ],
  },
  {
    id: 'backup',
    title: 'Backup and export',
    blocks: [
      {
        t: 'ul',
        items: [
          '**Backup:** one encrypted `.meowbak` file, locked with a backup passphrase. It can include the documents. Back up now, or every day, week or month to a folder you choose.',
          '**Restore** replaces all your data with the backup, from settings or from the first-run screen. A restore that fails changes nothing.',
          '**Export** to JSON, CSV, Excel or PDF. You choose the period, categories and parts, and see a preview first. **Export files are not encrypted:** keep your data safe with a backup.',
          '**Delete all my data** removes the database, its key and the backups the app made.',
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
          '**Encrypted at rest:** one SQLCipher database. The key is in your keychain, optionally behind a passphrase, and a recovery key is shown once.',
          '**Locks itself** after 1, 5 or 15 minutes or an hour of no use (or never), and optionally when minimised or closed. Locking drops the key from memory.',
          '**Nothing leaves your devices:** no account, no telemetry, no cloud. A check in CI lists every place in the code that can open a connection, and fails when a new one appears.',
          '**A hardened window:** a strict content security policy, no remote content, and links open in your browser.',
          '**Safe logs:** logs and crash reports never hold amounts, descriptions, tokens or keys.',
        ],
      },
      { t: 'p', text: `What is stored where, and who could see it: [the data inventory](${doc('data-inventory.md')}).` },
    ],
  },
  {
    id: 'shortcuts',
    title: 'Keyboard shortcuts',
    blocks: [
      { t: 'p', text: 'On the computer, press `?` to see them all. They are ignored while you type in a field.' },
      {
        t: 'table',
        head: ['Keys', 'What they do'],
        rows: [
          ['`g` then `d` `t` `c` `r`', 'go to the dashboard, transactions, categories, recurring'],
          ['`g` then `f` `o` `e` `s`', 'go to debts, documents, export, settings'],
          ['`n`', 'new'],
          ['`/`', 'search'],
          ['`[` `]`', 'previous and next month'],
          ['`t`', 'this month'],
          ['`j` `k`', 'next and previous row'],
          ['`Ctrl+L` (`Cmd+L`)', 'lock the app'],
          ['`Ctrl+B` (`Cmd+B`)', 'show or hide the sidebar'],
        ],
      },
    ],
  },
  {
    id: 'cli',
    title: 'Command line',
    blocks: [
      { t: 'p', text: '`meowny` drives the running desktop app from a terminal, a script or an AI agent. It never opens your data file: it asks the app over `127.0.0.1`, which is off until you turn it on, and every request needs a token you make in the app.' },
      {
        t: 'ol',
        items: [
          'In the app: **settings › command line access**. Turn it on and make a token (read only unless a script must change things).',
          'Build the program: `cargo install --path cli`.',
          'Give it the token once: `echo "$TOKEN" | meowny login`.',
        ],
      },
      { t: 'code', text: 'meowny status\nmeowny transactions list --limit 5\nmeowny stats budget --month 2026-10 --json\nmeowny debts summary' },
      { t: 'p', text: 'Exports, backups and copies of documents also need your yes in the app, each time. Every command, flag and exit code, with real output: [the command line reference](cli.html).' },
    ],
  },
]

export function buildGuide() {
  return {
    file: 'guide.html',
    title: 'meowNY guide',
    description: 'How meowNY works: budgets, recurring items, documents, debts and shared bills, the phone and its sync, backups, privacy and the command line.',
    lead: 'What each part of the app does, and how to use it.',
    footer: `meowNY is free software (AGPL-3.0). Source: [${REPO.replace('https://', '')}](${REPO}).`,
    nav: [{ title: 'Guide', links: SECTIONS.map((s) => ({ id: s.id, text: s.title })) }],
    sections: SECTIONS,
  }
}
