// The guide: what each part of meowNY does, in a few lines each. The Markdown guides in docs/ hold every detail; the
// sections link to them.

import { REPO } from './render.mjs'

const doc = (name) => `${REPO}/blob/main/docs/${name}`

const SECTIONS = [
  {
    id: 'start',
    title: 'Getting started',
    blocks: [
      { t: 'p', text: 'A budget app for your computer (Linux, macOS, Windows) and your phone (Android or iPhone). Your data stays on your devices, encrypted.' },
      { t: 'note', kind: 'info', text: '**Pre-release:** no installers yet. Build from source.' },
      { t: 'h3', id: 'install', text: 'Build and run' },
      { t: 'p', text: "You need Node.js 22+, Rust 1.90+, [Tauri's system libraries](https://v2.tauri.app/start/prerequisites/) and a system keychain." },
      { t: 'code', text: 'git clone https://github.com/v1nc3t/meowNY.git\ncd meowNY\nnpm install\nnpm run tauri dev' },
      { t: 'h3', id: 'first-run', text: 'First run' },
      { t: 'ol', items: ['Pick a currency.', 'Add a passphrase, if you want one.', '**Write down the recovery key.** It is shown once.'] },
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
    ],
  },
  {
    id: 'budgeting',
    title: 'Budgeting',
    blocks: [
      {
        t: 'ul',
        items: [
          '**Categories** sort transactions into income and expenses.',
          '**Budgets** per month and category. Past months keep theirs.',
          '**Recurring items:** pay, skip, pause or end them.',
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
          'Attach receipts and invoices to transactions, recurring items and debts.',
          'Stored encrypted, up to 25 MiB each.',
          'Preview images, PDF, text, CSV, Word and Excel inside the app.',
          '**Keep a copy on my phone** picks which ones go to the phone.',
        ],
      },
      { t: 'p', text: `More: [the documents guide](${doc('documents.md')}).` },
    ],
  },
  {
    id: 'debts',
    title: 'Debts and shared bills',
    blocks: [
      {
        t: 'ul',
        items: [
          'A friend is just a name.',
          '**Lend, borrow, paid back:** a ledger per friend, outside your budget.',
          '**Split a bill** equally, by percent, shares or amounts. Budgets count only your part.',
          '**Bill codes:** a QR code a friend scans to add their part.',
          '**Reminders:** every week, 2 weeks or month.',
        ],
      },
      { t: 'p', text: `More: [the debts guide](${doc('debts.md')}).` },
    ],
  },
  {
    id: 'phone',
    title: 'Your phone',
    blocks: [
      { t: 'p', text: 'The phone app (Android or iPhone) keeps a full copy and works offline. It syncs over your home wifi, with no server.' },
      { t: 'h3', id: 'pairing', text: 'Pair once' },
      { t: 'ol', items: ['Computer: **settings › phone › add a phone**.', 'Phone: scan the QR code.', 'Type the six digits the computer shows.'] },
      { t: 'h3', id: 'syncing', text: 'Sync' },
      {
        t: 'ul',
        items: [
          'Computer: **settings › phone › sync with phone**, then scan.',
          'Choose a direction: both ways, to the computer or to the phone.',
          'Conflicts are shown side by side; you pick.',
          'Both devices must be on the same network.',
          'iPhone: allow **local network** when asked, then scan again.',
          'A passphrase locks the phone. Five wrong tries erase it; pair again to restore.',
        ],
      },
      { t: 'h3', id: 'get-apk', text: 'Get the app' },
      { t: 'p', text: `**Android:** download the debug APK from the **Android** workflow in GitHub Actions, or [build it yourself](${doc('development.md')}#android).` },
      { t: 'p', text: `**iPhone:** download the IPA from the **iOS** workflow and install it with [AltStore](https://altstore.io) or [SideStore](https://sidestore.io). A free Apple ID needs a refresh every 7 days. Documents are previewed in the app; iPhone has no "open". [Build it yourself](${doc('development.md')}#ios).` },
    ],
  },
  {
    id: 'backup',
    title: 'Backup and export',
    blocks: [
      {
        t: 'ul',
        items: [
          '**Backup:** one encrypted file, now or on a schedule. Can include documents.',
          '**Restore** from settings or the first-run screen.',
          '**Export** to JSON, CSV, Excel or PDF. Exports are not encrypted.',
          '**Delete all my data** wipes everything.',
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
          'Encrypted database; the key is in your system keychain.',
          'Locks itself when idle and forgets the key.',
          'No account, no telemetry, no cloud.',
          'Logs never hold amounts, descriptions or keys.',
        ],
      },
      { t: 'p', text: `Details: [the data inventory](${doc('data-inventory.md')}).` },
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
      { t: 'p', text: '`meowny` lets scripts and AI agents use the running desktop app.' },
      { t: 'code', text: 'cargo install --path cli\necho "$TOKEN" | meowny login\nmeowny status' },
      { t: 'p', text: 'Full reference: [command line](cli.html).' },
    ],
  },
]

export function buildGuide() {
  return {
    file: 'guide.html',
    title: 'meowNY guide',
    description: 'How meowNY works: budgets, documents, debts, the phone, backups, privacy and the command line.',
    lead: 'What each part of the app does.',
    footer: `meowNY is free software (AGPL-3.0). [Source](${REPO}).`,
    nav: [{ title: 'Guide', links: SECTIONS.map((s) => ({ id: s.id, text: s.title })) }],
    sections: SECTIONS,
  }
}
