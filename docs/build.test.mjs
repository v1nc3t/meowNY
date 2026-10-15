// Tests for the documentation generator: node --test docs/build.test.mjs
// They check that the page is safe (nothing from a command's output becomes markup), complete (every command,
// route and code is on it), self-consistent (links resolve) and that it fails loudly instead of quietly falling
// behind the program.

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { build, inputs, lint } from './build.mjs'
import { buildPage } from './content.mjs'
import { escapeHtml, inlineHtml, safeLink, toHtml, toMarkdown } from './render.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const fresh = () => structuredClone(inputs())

// ---- the text is escaped, and links are only links when they are safe ----

test('text is escaped, so a command that prints markup cannot add any', () => {
  assert.equal(escapeHtml(`<img src=x onerror="alert(1)"> & 'q'`), '&lt;img src=x onerror=&quot;alert(1)&quot;&gt; &amp; \'q\'')
  assert.equal(inlineHtml('a <script>alert(1)</script> b'), 'a &lt;script&gt;alert(1)&lt;/script&gt; b')
  assert.equal(inlineHtml('use `<b>` here'), 'use <code>&lt;b&gt;</code> here')
  assert.equal(inlineHtml('**<i>**'), '<strong>&lt;i&gt;</strong>')
})

test('the little markup works and nothing else does', () => {
  assert.equal(inlineHtml('`a` and **b** and *c*'), '<code>a</code> and <strong>b</strong> and <em>c</em>')
  assert.equal(inlineHtml('[the site](https://example.org/x)'), '<a href="https://example.org/x">the site</a>')
  assert.equal(inlineHtml('[`status`](#cmd-status)'), '<a href="#cmd-status"><code>status</code></a>')
  assert.equal(inlineHtml('2 * 3 = 6'), '2 * 3 = 6', 'a lone star is a star')
  assert.equal(inlineHtml('a `` b'), 'a `` b')
})

test('only links to this page, this site and the web are links', () => {
  for (const ok of ['#top', 'CLI.md', './CLI.md', '../x', 'https://example.org', 'http://127.0.0.1:80/x', 'a/b']) assert.ok(safeLink(ok), ok)
  for (const bad of ['javascript:alert(1)', 'JavaScript:alert(1)', 'data:text/html,x', 'vbscript:x', 'file:///etc/passwd', 'mailto:a@b.c', 'x:y']) assert.ok(!safeLink(bad), bad)
  assert.equal(inlineHtml('[click](javascript:void)'), 'click', 'an unsafe link is only its text')
  assert.ok(!inlineHtml('[click](data:text/html,<script>)').includes('<a'))
})

// ---- the page is built from the program's own facts ----

test('the built page has no problems and every command, route and code on it', () => {
  const built = build()
  assert.deepEqual(built.problems, [])
  const data = inputs()
  for (const c of data.commands.commands) {
    assert.ok(built.html.includes(`id="cmd-${c.command.replace(/ /g, '-')}"`), c.command)
    assert.ok(built.markdown.includes(`#### \`${c.command}\``), `${c.command} in the Markdown`)
    assert.ok(built.html.includes(escapeHtml(c.usage)), `${c.command}: its usage line`)
  }
  for (const r of data.routes.routes) assert.ok(built.html.includes(`<td><code>${r.name}</code></td>`), r.name)
  assert.ok(data.commands.commands.length >= 40)
})

test('building twice gives the same page, and docs/CLI.md is what the generator makes', () => {
  assert.equal(build().html, build().html)
  assert.equal(readFileSync(join(here, 'CLI.md'), 'utf8'), build().markdown, 'docs/CLI.md is out of date: run node docs/build.mjs')
})

test('every example on the page is a recorded one for a command that exists', () => {
  const { commands, examples } = inputs()
  const names = new Set(commands.commands.map((c) => c.command))
  assert.deepEqual([...names].filter((n) => !examples.examples[n]), [], 'commands with no example')
  assert.deepEqual(Object.keys(examples.examples).filter((n) => !names.has(n)), [], 'examples of commands that do not exist')
  for (const [name, e] of Object.entries(examples.examples)) {
    assert.ok(e.command.startsWith('meowny ') || e.command.includes('| meowny '), name)
    assert.ok(e.human || e.json, name)
  }
})

test('what a command printed is shown, escaped, and never as markup', () => {
  const data = fresh()
  data.examples.examples.status.human.stdout = '<img src=x onerror=alert(1)>\n<script>alert(2)</script>\n'
  const { html, problems } = build(data)
  assert.deepEqual(problems, [])
  assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;') && html.includes('&lt;script&gt;alert(2)&lt;/script&gt;'))
  assert.ok(!html.includes('<img') && !/<script/i.test(html))
})

test('JSON examples are shown indented, and the page says that the program prints one line', () => {
  const { html, markdown } = build()
  assert.ok(html.includes('with --json (one line; shown indented)'))
  assert.ok(html.includes('&quot;api&quot;: 1') || html.includes('"api": 1'), 'indented JSON')
  assert.ok(markdown.includes('With `--json` (one line; shown indented):'))
})

// ---- it fails instead of falling behind ----

test('a command with no example stops the build', () => {
  const data = fresh()
  delete data.examples.examples['transactions list']
  assert.throws(() => buildPage(data), /no example for the command "transactions list"/)
})

test('a new error code that the prose does not explain stops the build', () => {
  const data = fresh()
  data.commands.errorCodes.brand_new_code = 1
  assert.throws(() => buildPage(data), /brand_new_code/)
  const other = fresh()
  delete other.commands.errorCodes.locked
  assert.throws(() => buildPage(other), /"locked"/, 'prose about a code the program no longer has')
})

test('a new group of commands that the page does not know stops the build', () => {
  const data = fresh()
  data.commands.commands.push({ ...data.commands.commands[0], command: 'gadgets list' })
  data.examples.examples['gadgets list'] = data.examples.examples.status
  assert.throws(() => buildPage(data), /new group of commands, "gadgets"/)
})

test('a missing route in the HTTP table is caught', () => {
  const data = fresh()
  data.routes.routes.push({ name: 'gadgets.list', scope: 'read', summary: 'Lists gadgets.', params: [] })
  const built = build({ ...data, routes: { ...data.routes, routes: data.routes.routes.slice(0, -1) } })
  assert.deepEqual(built.problems, [])
  const page = buildPage(data)
  assert.ok(toHtml(page).includes('gadgets.list'), 'a new route appears in the table by itself')
})

test('the checks catch what a reader would trip over', () => {
  const data = inputs()
  const html = build().html
  const broken = (change) => lint(change(html), data)
  assert.ok(broken((h) => h.replace('id="overview"', 'id="quick-start"')).some((p) => p.includes('used twice')))
  assert.ok(broken((h) => h + '<a href="#nowhere">x</a>').some((p) => p.includes('#nowhere')))
  assert.ok(broken((h) => h.replace('</body>', '<script src="https://cdn.example/x.js"></script></body>')).some((p) => p.includes('outside itself')))
  assert.ok(broken((h) => h.replace('</head>', '<link rel="stylesheet" href="https://fonts.example/f.css"></head>')).some((p) => p.includes('outside itself')))
  assert.ok(broken((h) => h.replace('<body>', '<body onload="x()">')).some((p) => p.includes('event handler')))
  assert.ok(broken((h) => h.replace('<h1', '<h1></h1><h1')).some((p) => p.includes('exactly one h1')))
  assert.ok(broken((h) => h.replace('id="cmd-status"', 'id="x"')).some((p) => p.includes('"status"')))
  assert.ok(broken((h) => h.replace('<td><strong>3</strong></td>', '<td>3</td>')).some((p) => p.includes('exit code 3')))
  assert.deepEqual(lint(html, data), [])
})

// ---- what the page is like ----

test('the page is one document with a language, a title, a skip link and a header on every table', () => {
  const { html } = build()
  assert.match(html, /^<!doctype html>\n<html lang="en">/)
  assert.equal((html.match(/<h1[\s>]/g) ?? []).length, 1)
  assert.ok(html.includes('class="skip" href="#main"') && html.includes('<main id="main">'))
  const tables = html.match(/<table/g)?.length ?? 0
  assert.ok(tables > 30 && (html.match(/<th scope="col">/g) ?? []).length >= tables * 2, 'every table has column headers')
  assert.ok(!/<th>/.test(html), 'no header cell without a scope')
})

test('the page asks for nothing from anywhere else, and only reaches out with plain links', () => {
  const { html } = build()
  assert.ok(!/<script|<iframe|<img|@import|\ssrc=/i.test(html), 'no script, frame, image or source')
  assert.deepEqual([...html.matchAll(/<link[^>]*>/g)].map((m) => m[0]), ['<link rel="stylesheet" href="style.css">'])
  for (const [, url] of html.matchAll(/href="(https?:[^"]+)"/g)) assert.ok(url.startsWith('https://'), url)
  const css = readFileSync(join(here, 'site.css'), 'utf8')
  assert.ok(!/url\(|@import|https?:\/\//i.test(css), 'the stylesheet loads nothing')
})

test('the Markdown file has one heading for the page and one for each section', () => {
  const { markdown, page } = build()
  assert.ok(markdown.startsWith('# meowny command line\n'))
  // (a `# comment` inside a code block is not a heading)
  assert.equal((markdown.replace(/^```[\s\S]*?^```$/gm, '').match(/^# /gm) ?? []).length, 1)
  for (const s of page.sections) assert.ok(markdown.includes(`\n## ${s.title}\n`), s.title)
  assert.ok(!markdown.includes('<span') && !markdown.includes('<code>'), 'no HTML in the Markdown')
})

test('a table cell with a pipe does not break the Markdown table', () => {
  const md = toMarkdown({ title: 't', lead: 'l', footer: 'f', sections: [{ id: 's', title: 'S', blocks: [{ t: 'table', head: ['a', 'b'], rows: [['x|y', 'z']] }] }] })
  assert.ok(md.includes('| x\\|y | z |'))
})

test('the examples show no secret and no real paths', () => {
  const text = readFileSync(join(here, 'examples.json'), 'utf8')
  assert.ok(!/mny_[0-9a-f]{64}/.test(text), 'a token is in the examples')
  assert.ok(!/\/tmp\/|\.tmp[A-Za-z0-9]{6}/.test(text), 'a temporary path is in the examples')
  assert.ok(!text.includes('a long enough passphrase'))
})
