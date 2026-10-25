// The landing page: what meowNY is, in one screen, with links into the guide and the command line reference.
//
// Screenshots: shots.mjs; one whose files are not in docs/site/images/ yet shows as an empty frame.

import { readFileSync } from 'node:fs'
import { escapeHtml, icon, inlineHtml, REPO, shell, SPONSOR } from './render.mjs'
import { SHOTS, shot } from './shots.mjs'

export { SHOTS }

/** The app's version, from the one place it is set (scripts/release.mjs changes it). */
const VERSION = JSON.parse(readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).version

const FEATURES = [
  { icon: 'budgets', title: 'budgets', text: 'Monthly budgets per category.', href: 'guide.html#budgeting' },
  { icon: 'recurring', title: 'recurring', text: 'Rent, salary, subscriptions.', href: 'guide.html#budgeting' },
  { icon: 'home', title: 'statistics', text: 'Spending, trends, top categories.', href: 'guide.html#budgeting' },
  { icon: 'documents', title: 'documents', text: 'Receipts, kept encrypted.', href: 'guide.html#documents' },
  { icon: 'debts', title: 'debts', text: 'Who owes whom. Split bills.', href: 'guide.html#debts' },
  { icon: 'sync', title: 'phone', text: 'Android, synced over wifi.', href: 'guide.html#phone' },
  { icon: 'export', title: 'backup', text: 'Encrypted backups and exports.', href: 'guide.html#backup' },
  { icon: 'terminal', title: 'AI agents, if you want', text: 'Let an assistant add and sort your expenses.', href: '#agents' },
]

const AGENT = [
  ['off until you say so', 'One switch in **settings › CLI access**. meowNY works the same without it.'],
  ['you set the limits', 'Give the agent a read-only key, or one that may add and change things. Keys expire, and you can revoke one any time.'],
  ['files need your yes', 'Exports, backups and copies of documents wait for you to click Allow. It listens only on your own computer.'],
  ['your choice of agent', 'An agent that runs in the cloud sees what it reads. Use a read-only key, or a model on your own computer, if that matters to you.'],
]

const PRIVATE = [
  ['encrypted at rest', 'One SQLCipher file. The key sits in your keychain, optionally behind a passphrase, and a recovery key is shown once.'],
  ['locks itself', 'After the idle time you choose, and on minimise or close if you like. Locking drops the key from memory.'],
  ['nothing leaves', 'No account, no telemetry, no cloud. A check in CI fails when new network code appears.'],
  ['sync without a cloud', 'The phone connects straight to the computer on your wifi, after a QR code and six typed digits.'],
]

/** What the gallery shows, in order: the desktop first, then a row of the phone. */
const DESKTOP_SHOTS = ['transactions', 'split', 'bill-code', 'debts', 'friend', 'preview-pdf', 'categories', 'recurring', 'export']
const PHONE_SHOTS = ['phone-dashboard', 'phone-transactions', 'phone-quick-add', 'phone-debts']

/** The landing page. `images` holds the names of the screenshots that exist. */
export function landingHtml({ images = new Set() } = {}) {
  const features = FEATURES.map(
    (f) => `<a class="card feature" href="${f.href}"><span class="feature-icon">${icon(f.icon)}</span><h3>${escapeHtml(f.title)}</h3><p>${escapeHtml(f.text)}</p></a>`,
  ).join('\n')
  const agentRows = AGENT.map(([title, text]) => `<li><strong>${escapeHtml(title)}</strong><span>${inlineHtml(text)}</span></li>`).join('')
  const rows = PRIVATE.map(([title, text]) => `<li><strong>${escapeHtml(title)}</strong><span>${escapeHtml(text)}</span></li>`).join('')

  const body = `<header class="hero landing" id="top">
<p><span class="badge">pre-release ${VERSION}</span></p>
<h1>meowNY</h1>
<p class="lead">A budget app that keeps your money data on your own computer and phone, encrypted. No server, no account, no telemetry.</p>
<p class="actions"><a class="btn btn-primary btn-lg" href="${REPO}/releases/latest">download</a><a class="btn btn-outline btn-lg" href="guide.html">read the guide</a><a class="btn btn-ghost btn-lg" href="cli.html">command line</a><a class="btn btn-ghost btn-lg" href="${REPO}">source on github</a></p>
</header>
${shot('dashboard', images, { eager: true })}
<section class="block" id="features"><h2>features</h2>
<div class="grid">
${features}
</div>
</section>
<section class="block" id="look"><h2>a look inside</h2>
<div class="grid shots">${DESKTOP_SHOTS.map((n) => shot(n, images)).join('')}</div>
</section>
<section class="block" id="phone"><h2>on your phone</h2>
<p>Android today, iPhone later. A full copy that works offline and syncs straight to the computer over your wifi.</p>
<div class="grid shots phone">${PHONE_SHOTS.map((n) => shot(n, images)).join('')}</div>
</section>
<section class="block" id="agents"><h2>an AI agent can help, if you want</h2>
<p>Ask an assistant like Claude to type in last month's receipts, sort a bank statement into categories, or tell you what you spent on food in March. It works through <code>meowny</code>, the command line that talks to the app on your computer.</p>
<ul class="rows">${agentRows}</ul>
<p class="actions"><a class="btn btn-outline" href="cli.html#quick-start">set it up</a><a class="btn btn-ghost" href="cli.html#agents">instructions for the agent</a></p>
</section>
<section class="block" id="private"><h2>private by design</h2>
<ul class="rows">${rows}</ul>
<p class="small">More in the guide: <a href="guide.html#privacy">privacy and security</a>.</p>
</section>
<section class="block" id="start"><h2>get started</h2>
<p>Download meowNY for Linux, Windows or Android from <a href="${REPO}/releases/latest">the latest release</a>. Step by step: <a href="guide.html#install">install</a>. iPhone and Mac are not available yet: <a href="#support">help make it happen</a>.</p>
<p>To build it yourself: <a href="develop.html">develop</a>. The phone app and its sync: <a href="guide.html#phone">your phone</a>.</p>
</section>
<section class="block" id="support"><h2>support meowNY</h2>
<p>meowNY is free and stays free. Putting it on the App Store and Google Play costs money: the developer accounts and the yearly fees. Support on Ko-fi goes toward publishing it there, so anyone can install it from their phone's store.</p>
<p class="actions"><a class="btn btn-outline" href="${SPONSOR}">support on ko-fi</a></p>
</section>
<footer><p>meowNY is free software under the AGPL-3.0. <a href="${REPO}">Source on GitHub</a> · <a href="${SPONSOR}">Support on Ko-fi</a>.</p></footer>`

  return shell({
    file: 'index.html',
    title: 'meowNY: a private, local-first budget app',
    description: 'meowNY is a budget app that keeps your data on your own computer and phone, encrypted. No server, no account, no telemetry.',
    toc: [{ title: 'On this page', links: [{ id: 'features', text: 'features' }, { id: 'look', text: 'a look inside' }, { id: 'phone', text: 'on your phone' }, { id: 'agents', text: 'AI agents' }, { id: 'private', text: 'private by design' }, { id: 'start', text: 'get started' }, { id: 'support', text: 'support' }] }],
    body,
    wide: true,
  })
}
