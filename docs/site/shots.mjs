// The screenshots of the site. Each one is two files in docs/site/images/, `<name>-light.png` and `<name>-dark.png`, taken
// from the real app with its demo data (src-tauri/seeds); the page shows the one that matches the reader's theme. A shot
// whose files are not there yet is an empty frame on the landing page and left out of the guide.

import { escapeHtml } from './render.mjs'

const DESKTOP = { width: 1280, height: 800 }
const PHONE = { width: 400, height: 860, phone: true }

export const SHOTS = {
  dashboard: { ...DESKTOP, alt: 'The dashboard: income, expenses and savings of the month, a year of trends and the budgets', caption: 'the dashboard' },
  transactions: { ...DESKTOP, alt: 'The transactions of September with filters by month, text and category', caption: 'transactions' },
  split: { ...DESKTOP, alt: 'A taxi bill split equally between you and two friends; the odd cent goes to you', caption: 'splitting a bill' },
  'bill-code': { ...DESKTOP, alt: "A QR code with Marta's part of a shared bill, for her to scan", caption: 'a bill code for a friend' },
  categories: { ...DESKTOP, alt: 'Budgets of expenses and income side by side, with the budget history', caption: 'categories and budgets' },
  recurring: { ...DESKTOP, alt: 'Recurring items with mark as paid, skip and pause; an overdue one is marked', caption: 'recurring items' },
  debts: { ...DESKTOP, alt: 'Friends with what each one owes, and the reminders that have come', caption: 'debts' },
  'debt-history': { ...DESKTOP, alt: 'The history of debts: shared bills, repayments and an overdue payment, with balances', caption: 'history of debts' },
  friend: { ...DESKTOP, alt: "Marta's page: what she owes and every line of her ledger", caption: "a friend's ledger" },
  documents: { ...DESKTOP, alt: 'The documents page with a PDF receipt, a CSV and a text file', caption: 'documents' },
  'preview-pdf': { ...DESKTOP, alt: 'A PDF receipt previewed inside the app', caption: 'a receipt, previewed' },
  'preview-csv': { ...DESKTOP, alt: 'A CSV of electricity bills previewed as a sheet', caption: 'a spreadsheet, previewed' },
  export: { ...DESKTOP, alt: 'The export page: format, period and filters on the left, a preview of the file on the right', caption: 'export' },
  settings: { ...DESKTOP, alt: 'Settings: pairing a phone, with the direction of the sync', caption: 'settings' },
  lock: { ...DESKTOP, alt: 'The lock screen asking for the passphrase', caption: 'the lock screen' },
  'phone-dashboard': { ...PHONE, alt: 'The dashboard on the phone', caption: 'dashboard' },
  'phone-transactions': { ...PHONE, alt: 'Transactions on the phone', caption: 'transactions' },
  'phone-recurring': { ...PHONE, alt: 'Recurring items on the phone', caption: 'recurring' },
  'phone-debts': { ...PHONE, alt: 'Debts on the phone, with scan for a bill code', caption: 'debts' },
  'phone-categories': { ...PHONE, alt: 'Categories and budgets on the phone', caption: 'budgets' },
  'phone-quick-add': { ...PHONE, alt: 'The add button on the phone: a transaction, a category, a friend or a scanned bill', caption: 'quick add' },
  'phone-sync': { ...PHONE, alt: 'The phone asking to scan the code the computer shows', caption: 'sync with your computer' },
  'phone-settings': { ...PHONE, alt: 'Settings on the phone', caption: 'settings' },
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
