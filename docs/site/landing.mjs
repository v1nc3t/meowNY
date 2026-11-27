// The landing page: what meowNY is, in one screen, with links into the guide and the command line reference.
//
// Screenshots: shots.mjs; one whose files are not in docs/site/images/ yet shows as an empty frame.

import { readFileSync } from 'node:fs'
import { escapeHtml, REPO, shell, SPONSOR } from './render.mjs'
import { SHOTS, shot } from './shots.mjs'

export { SHOTS }

/** The app's version, from the one place it is set (scripts/release.mjs changes it). */
const VERSION = JSON.parse(readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).version

/** What it does: a name, a line, and where the guide says more. */
const FEATURES = [
  ['budgets', 'Monthly budgets per category, with statistics and trends.', 'guide.html#budgeting'],
  ['dashboard', 'A dashboard you arrange, with 18 widgets.', 'guide.html#dashboard-customize'],
  ['accounts', 'Bank, savings, cash and credit card, in any currency, with transfers and net worth.', 'guide.html#accounts'],
  ['stashes', 'Money set aside for a goal, or a term deposit at a rate.', 'guide.html#stashes'],
  ['recurring', 'Rent, salary, subscriptions.', 'guide.html#recurring'],
  ['documents', 'Receipts, kept encrypted.', 'guide.html#documents'],
  ['debts', 'Who owes whom. Split bills, with a QR code for each friend. Loans and mortgages.', 'guide.html#debts'],
  ['phone', 'Android, synced from your computer over wifi. No cloud.', 'guide.html#phone'],
  ['backup', 'Encrypted backups. Exports to JSON, CSV, Excel and PDF.', 'guide.html#backup'],
  ['private', 'Encrypted at rest, locks itself, and nothing leaves the device.', 'guide.html#privacy'],
  ['AI agents, if you want', 'An assistant can add and sort your expenses through the command line. Off until you turn it on.', 'cli.html#quick-start'],
]

/** The landing page. `images` holds the names of the screenshots that exist. */
export function landingHtml({ images = new Set() } = {}) {
  const features = FEATURES.map(([title, text, href]) => `<li><a href="${href}">${escapeHtml(title)}</a><span>${escapeHtml(text)}</span></li>`).join('\n')

  const body = `<header class="hero" id="top">
<h1>meowNY</h1>
<p class="lead">A budget app that keeps your money data on your own computer and phone, encrypted. No server, no account, no telemetry.</p>
<p class="actions"><a class="btn" href="${REPO}/releases/latest">download</a><a href="guide.html">read the guide</a><a href="cli.html">command line</a></p>
<p class="small">pre-release ${VERSION}: Linux, Windows and Android.</p>
</header>
${shot('dashboard', images, { eager: true })}
<section class="doc" id="features"><h2>features</h2>
<ul class="features">
${features}
</ul>
</section>
<section class="doc" id="start"><h2>get started</h2>
<ol>
<li>Download meowNY for Linux, Windows or Android from <a href="${REPO}/releases/latest">the latest release</a>.</li>
<li>Install it: <a href="guide.html#install">step by step</a>.</li>
<li>Open it and press <strong>get started</strong>.</li>
</ol>
<p>iPhone and Mac are not available yet: <a href="#support">help make it happen</a>. To build it yourself: <a href="develop.html">develop</a>.</p>
</section>
<section class="doc" id="support"><h2>support meowNY</h2>
<p>meowNY is free and stays free. The App Store and Google Play need paid developer accounts. <a href="${SPONSOR}">Support on Ko-fi</a> goes toward publishing it there.</p>
</section>
<footer><p>meowNY is free software under the AGPL-3.0. <a href="${REPO}">Source on GitHub</a> · <a href="${SPONSOR}">Support on Ko-fi</a>.</p></footer>`

  return shell({
    file: 'index.html',
    title: 'meowNY: a private, local-first budget app',
    description: 'meowNY is a budget app that keeps your data on your own computer and phone, encrypted. No server, no account, no telemetry.',
    body,
    wide: true,
  })
}
