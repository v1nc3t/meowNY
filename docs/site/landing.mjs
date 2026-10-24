// The landing page: what meowNY is, in one screen, with links into the guide and the command line reference.
//
// Screenshots: a picture named in SHOTS that exists as docs/site/images/<name>.png is shown; until then the page shows
// an empty frame with its name.

import { readFileSync } from 'node:fs'
import { escapeHtml, icon, REPO, shell } from './render.mjs'

/** The app's version, from the one place it is set (scripts/release.mjs changes it). */
const VERSION = JSON.parse(readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).version

export const SHOTS = {
  dashboard: { alt: "The dashboard: this month's budget, spending by category and what is due", caption: 'the dashboard' },
  transactions: { alt: 'The transactions page with filters', caption: 'transactions' },
  debts: { alt: 'The debts page: friends and what each one owes', caption: 'debts and shared bills' },
  phone: { alt: 'The phone app beside the desktop app', caption: 'phone and computer' },
}

const FEATURES = [
  { icon: 'budgets', title: 'budgets', text: 'Monthly budgets per category.', href: 'guide.html#budgeting' },
  { icon: 'recurring', title: 'recurring', text: 'Rent, salary, subscriptions.', href: 'guide.html#budgeting' },
  { icon: 'home', title: 'statistics', text: 'Spending, trends, top categories.', href: 'guide.html#budgeting' },
  { icon: 'documents', title: 'documents', text: 'Receipts, kept encrypted.', href: 'guide.html#documents' },
  { icon: 'debts', title: 'debts', text: 'Who owes whom. Split bills.', href: 'guide.html#debts' },
  { icon: 'sync', title: 'phone', text: 'Android and iPhone, synced over wifi.', href: 'guide.html#phone' },
  { icon: 'export', title: 'backup', text: 'Encrypted backups and exports.', href: 'guide.html#backup' },
  { icon: 'terminal', title: 'command line', text: 'For scripts and AI agents.', href: 'cli.html' },
]

const PRIVATE = [
  ['encrypted at rest', 'One SQLCipher file. The key sits in your keychain, optionally behind a passphrase, and a recovery key is shown once.'],
  ['locks itself', 'After the idle time you choose, and on minimise or close if you like. Locking drops the key from memory.'],
  ['nothing leaves', 'No account, no telemetry, no cloud. A check in CI fails when new network code appears.'],
  ['sync without a cloud', 'The phone connects straight to the computer on your wifi, after a QR code and six typed digits.'],
]

function shot(name, images) {
  const s = SHOTS[name]
  const picture = images.has(name)
    ? `<img src="images/${name}.png" alt="${escapeHtml(s.alt)}" loading="lazy">`
    : `<div class="shot-frame" role="img" aria-label="${escapeHtml(`Screenshot to come: ${s.alt.toLowerCase()}`)}"><span>screenshot to come</span></div>`
  return `<figure class="shot">${picture}<figcaption>${escapeHtml(s.caption)}</figcaption></figure>`
}

/** The landing page. `images` holds the names of the screenshots that exist. */
export function landingHtml({ images = new Set() } = {}) {
  const features = FEATURES.map(
    (f) => `<a class="card feature" href="${f.href}"><span class="feature-icon">${icon(f.icon)}</span><h3>${escapeHtml(f.title)}</h3><p>${escapeHtml(f.text)}</p></a>`,
  ).join('\n')
  const rows = PRIVATE.map(([title, text]) => `<li><strong>${escapeHtml(title)}</strong><span>${escapeHtml(text)}</span></li>`).join('')

  const body = `<header class="hero landing" id="top">
<p><span class="badge">pre-release ${VERSION}</span></p>
<h1>meowNY</h1>
<p class="lead">A budget app that keeps your money data on your own computer and phone, encrypted. No server, no account, no telemetry.</p>
<p class="actions"><a class="btn btn-primary btn-lg" href="${REPO}/releases/latest">download</a><a class="btn btn-outline btn-lg" href="guide.html">read the guide</a><a class="btn btn-ghost btn-lg" href="cli.html">command line</a><a class="btn btn-ghost btn-lg" href="${REPO}">source on github</a></p>
</header>
${shot('dashboard', images)}
<section class="block" id="features"><h2>features</h2>
<div class="grid">
${features}
</div>
</section>
<section class="block" id="look"><h2>a look inside</h2>
<div class="grid shots">${['transactions', 'debts', 'phone'].map((n) => shot(n, images)).join('')}</div>
</section>
<section class="block" id="private"><h2>private by design</h2>
<ul class="rows">${rows}</ul>
<p class="small">More in the guide: <a href="guide.html#privacy">privacy and security</a>.</p>
</section>
<section class="block" id="start"><h2>get started</h2>
<p>Download meowNY for Linux, Windows, Android or iPhone from <a href="${REPO}/releases/latest">the latest release</a>. How to install and update each one: <a href="guide.html#install">install</a>.</p>
<p>To build it yourself: <a href="develop.html">develop</a>. The phone app and its sync: <a href="guide.html#phone">your phone</a>.</p>
</section>
<footer><p>meowNY is free software under the AGPL-3.0. <a href="${REPO}">Source on GitHub</a>.</p></footer>`

  return shell({
    file: 'index.html',
    title: 'meowNY: a private, local-first budget app',
    description: 'meowNY is a budget app that keeps your data on your own computer and phone, encrypted. No server, no account, no telemetry.',
    toc: [{ title: 'On this page', links: [{ id: 'features', text: 'features' }, { id: 'look', text: 'a look inside' }, { id: 'private', text: 'private by design' }, { id: 'start', text: 'get started' }] }],
    body,
    wide: true,
  })
}
