// The screenshots of the site. Each one is two files in docs/site/images/, `<name>-light.png` and `<name>-dark.png`: the app's own
// UI (ui/) running in a browser against a fake backend with made-up data (ui/harness.html, a local test file kept out of git), at
// 1280 x 800 for the computer and 400 x 860 (`?profile=phone`) for the phone, once with a light and once with a dark system theme.
// The page shows the one that matches the reader's theme. A shot whose files are not there yet is an empty frame on the landing
// page and left out of the guide. Retake them whenever a screen changes.

import { escapeHtml } from './render.mjs'

const DESKTOP = { width: 1280, height: 800 }
const PHONE = { width: 400, height: 860, phone: true }

export const SHOTS = {
  dashboard: { ...DESKTOP, alt: 'The dashboard: income, expenses and savings of the month, a year of trends, the budgets and net worth', caption: 'the dashboard' },
  transactions: { ...DESKTOP, alt: 'The transactions of a month with the tabs all, expenses, income and transfers, and filters by text, category and account', caption: 'transactions' },
  'transactions-transfers': { ...DESKTOP, alt: 'The transfers tab: each transfer with its date, the account it came from, the account it went to, the amount and a note', caption: 'the transfers tab' },
  'add-income': { ...DESKTOP, alt: 'The add dialog on the income tab: a name, an amount, an income category and the account the money comes into', caption: 'adding an income' },
  split: { ...DESKTOP, alt: 'The add dialog on the expense tab: a taxi bill split equally between you and two friends; the odd cent goes to you', caption: 'splitting a bill' },
  'bill-code': { ...DESKTOP, alt: "A QR code with Anna's part of a shared bill, for her to scan", caption: 'a bill code for a friend' },
  accounts: { ...DESKTOP, alt: 'The accounts under the headings of their groups, with the type, last activity, change this month and balance of each, and the total in accounts', caption: 'accounts' },
  account: { ...DESKTOP, alt: 'One account: its balance over the year as a chart, what it spent and what came in by category for a month, and the lines of the month', caption: 'one account' },
  stashes: { ...DESKTOP, alt: 'The stashes: a card for each with the account it lives in, a bar, what is saved of the target, what is left or the amount a month, and add and take out buttons; the total saved is above them', caption: 'stashes' },
  stash: { ...DESKTOP, alt: 'One stash: how far it is, its notes, and a chart of what it held over the year with its target as a dashed line; the history of what was added and taken out is under it', caption: 'one stash' },
  categories: { ...DESKTOP, alt: 'The categories page: the expense categories of a month with what was spent and the budget of each; the income categories and the budget history follow under them', caption: 'categories and budgets' },
  recurring: { ...DESKTOP, alt: 'Recurring expenses and recurring income, each in a list of its own with mark as paid, skip and pause; an overdue one is marked', caption: 'recurring items' },
  debts: { ...DESKTOP, alt: 'The friends with what each one owes, and the history of every change of every debt', caption: 'debts' },
  friend: { ...DESKTOP, alt: "Anna's page: what she owes and every line of her ledger", caption: "a friend's ledger" },
  documents: { ...DESKTOP, alt: 'The documents page with receipts, PDFs, a CSV and a text file, and the ones that are kept on the phone', caption: 'documents' },
  'preview-pdf': { ...DESKTOP, alt: 'A PDF previewed inside the app, page by page', caption: 'a receipt, previewed' },
  'preview-csv': { ...DESKTOP, alt: 'A CSV file previewed as a sheet', caption: 'a spreadsheet, previewed' },
  export: { ...DESKTOP, alt: 'The export page: format, period and filters on the left, a preview of the file on the right', caption: 'export' },
  settings: { ...DESKTOP, alt: 'Settings, the phone part: the phones that are paired, and the direction of the sync', caption: 'settings' },
  lock: { ...DESKTOP, alt: 'The lock screen asking for the passphrase', caption: 'the lock screen' },
  'phone-dashboard': { ...PHONE, alt: 'The dashboard on the phone', caption: 'dashboard' },
  'phone-transactions': { ...PHONE, alt: 'Transactions on the phone, with the tabs all, expenses, income and transfers', caption: 'transactions' },
  'phone-recurring': { ...PHONE, alt: 'Recurring expenses and recurring income on the phone', caption: 'recurring' },
  'phone-debts': { ...PHONE, alt: 'Debts on the phone, with scan for a bill code', caption: 'debts' },
  'phone-accounts': { ...PHONE, alt: 'Accounts on the phone under the headings of their groups, each with its balance', caption: 'accounts' },
  'phone-stashes': { ...PHONE, alt: 'Stashes on the phone: a card for each with its bar, what is saved of the target and add and take out buttons', caption: 'stashes' },
  'phone-categories': { ...PHONE, alt: 'Expense categories and budgets on the phone, with the tabs expenses, income and budget history', caption: 'budgets' },
  'phone-quick-add': { ...PHONE, alt: 'The add button on the phone: a transaction, a category, a friend or a scanned bill', caption: 'quick add' },
  'phone-entry': { ...PHONE, alt: 'The add dialog on the phone, with the tabs expense, income and transfer', caption: 'add an entry' },
  'phone-sync': { ...PHONE, alt: 'The phone asking to scan the code the computer shows', caption: 'sync with your computer' },
  'phone-lock': { ...PHONE, alt: 'The lock screen on the phone', caption: 'locked' },
}

/** The files of a shot, as published under images/. */
export const shotFiles = (name) => [`${name}-light.png`, `${name}-dark.png`]

function picture(name, eager) {
  const s = SHOTS[name]
  const [light, dark] = shotFiles(name)
  return `<picture><source srcset="images/${dark}" media="(prefers-color-scheme: dark)"><img src="images/${light}" alt="${escapeHtml(s.alt)}" width="${s.width}" height="${s.height}"${eager ? '' : ' loading="lazy"'}></picture>`
}

/**
 * One screenshot with its caption; an empty frame (landing page) until its files exist. `eager` for one at the top of a page.
 * A click on it opens it big, in a popover (HTML's own, no script): a click outside it, a click on it or Escape closes it.
 */
export function shot(name, images, { eager = false } = {}) {
  const s = SHOTS[name]
  if (!s) throw new Error(`unknown screenshot ${name}`)
  const kind = s.phone ? ' is-phone' : ''
  if (!images.has(name)) {
    const frame = `<div class="shot-frame${kind}" role="img" aria-label="${escapeHtml(`Screenshot to come: ${s.alt.toLowerCase()}`)}"><span>screenshot to come</span></div>`
    return `<figure class="shot${kind}">${frame}<figcaption>${escapeHtml(s.caption)}</figcaption></figure>`
  }
  const view = `view-${name}`
  const open = `<button type="button" class="shot-open" popovertarget="${view}" aria-label="${escapeHtml(`Show bigger: ${s.caption}`)}">${picture(name, eager)}</button>`
  // The big one loads only when it is opened: an image in a closed popover is not shown, so lazy loading waits for it.
  const big = `<div class="shot-view${kind}" id="${view}" popover aria-label="${escapeHtml(s.caption)}"><button type="button" class="shot-close" popovertarget="${view}" popovertargetaction="hide" aria-label="Close">${picture(name, false).replace('<img ', '<img loading="lazy" ')}</button></div>`
  return `<figure class="shot${kind}">${open}<figcaption>${escapeHtml(s.caption)}</figcaption>${big}</figure>`
}

/** Rows of screenshots for the guide: the desktop ones, then the narrow phone ones; only those whose files exist. */
export function gallery(names, images) {
  const there = names.filter((n) => images.has(n))
  const row = (list, cls) => (list.length ? `<div class="grid shots${cls}">${list.map((n) => shot(n, images)).join('')}</div>` : '')
  return row(there.filter((n) => !SHOTS[n].phone), '') + row(there.filter((n) => SHOTS[n].phone), ' phone')
}
