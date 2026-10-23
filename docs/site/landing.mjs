// The landing page: what meowNY is, in one screen, with links into the guide and the command line reference.
//
// Screenshots: a picture named in SHOTS that exists as docs/site/images/<name>.png is shown; until then the page shows
// an empty frame with its name.

import { escapeHtml, icon, inlineHtml, REPO, shell } from './render.mjs'

export const SHOTS = {
  dashboard: { alt: "The dashboard: this month's budget, spending by category and what is due", caption: 'the dashboard' },
  transactions: { alt: 'The transactions page with filters', caption: 'transactions' },
  debts: { alt: 'The debts page: friends and what each one owes', caption: 'debts and shared bills' },
  phone: { alt: 'The phone app beside the desktop app', caption: 'phone and computer' },
}

const STATS = [
  ['servers', '0'],
  ['accounts', '0'],
  ['trackers', '0'],
  ['languages', '7'],
]

const FEATURES = [
  { icon: 'budgets', title: 'budgets', text: 'Categories, income and expenses, and a budget for each month. A new limit never rewrites past months.', href: 'guide.html#budgeting' },
  { icon: 'recurring', title: 'recurring', text: 'Rent, salary, subscriptions. See what is due, then pay, skip, pause or end it.', href: 'guide.html#budgeting' },
  { icon: 'home', title: 'statistics', text: 'Budget against spending, trends, a year view and your top categories.', href: 'guide.html#budgeting' },
  { icon: 'documents', title: 'documents', text: 'Receipts and invoices attached to transactions, stored encrypted, previewed inside the app.', href: 'guide.html#documents' },
  { icon: 'debts', title: 'debts and splitting', text: 'Who owes whom, and bills split any way you like. Budgets count only your part.', href: 'guide.html#debts' },
  { icon: 'sync', title: 'phone sync', text: 'An Android app with a full offline copy. It syncs with your computer over home wifi, with no server.', href: 'guide.html#phone' },
  { icon: 'export', title: 'backup and export', text: 'Encrypted backups on a schedule. Exports to JSON, CSV, Excel and PDF.', href: 'guide.html#backup' },
  { icon: 'terminal', title: 'command line', text: 'Scripts and AI agents use the running app through `meowny`, with tokens you make and revoke.', href: 'cli.html' },
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
  const stats = STATS.map(([label, value]) => `<div class="stat"><span class="stat-label">${label}</span><span class="stat-value">${value}</span></div>`).join('')
  const features = FEATURES.map(
    (f) => `<article class="card feature"><span class="feature-icon">${icon(f.icon)}</span><h3>${escapeHtml(f.title)}</h3><p>${inlineHtml(f.text)}</p><a class="more" href="${f.href}">more <span aria-hidden="true">›</span></a></article>`,
  ).join('\n')
  const rows = PRIVATE.map(([title, text]) => `<li><strong>${escapeHtml(title)}</strong><span>${escapeHtml(text)}</span></li>`).join('')

  const body = `<header class="hero landing" id="top">
<p><span class="badge">pre-release 0.1.0</span></p>
<h1>meowNY</h1>
<p class="lead">A budget app that keeps your money data on your own computer and phone, encrypted. No server, no account, no telemetry.</p>
<p class="actions"><a class="btn btn-primary btn-lg" href="guide.html">read the guide</a><a class="btn btn-outline btn-lg" href="cli.html">command line</a><a class="btn btn-ghost btn-lg" href="${REPO}">source on github</a></p>
</header>
${shot('dashboard', images)}
<section class="card stats" aria-label="At a glance">${stats}</section>
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
<p>No installers yet: build it from source with Node.js 22, Rust 1.90 and <a href="https://v2.tauri.app/start/prerequisites/">Tauri's system libraries</a>.</p>
<figure class="code"><pre><code>git clone https://github.com/v1nc3t/meowNY.git
cd meowNY
npm install
npm run tauri dev</code></pre></figure>
<p>Then follow <a href="guide.html#start">getting started</a> in the guide. The Android app and its sync: <a href="guide.html#phone">your phone</a>.</p>
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
