#!/usr/bin/env node
// Builds the documentation site for GitHub Pages, and docs/CLI.md, from the program's own description of itself.
//
//   node docs/site/build.mjs            write docs/CLI.md and the site in docs/site/_site/
//   node docs/site/build.mjs --check    build in memory, and fail if docs/CLI.md is out of date or the page has a problem
//
// No dependencies. The facts come from docs/site/commands.json and docs/site/examples.json (written by the program's tests,
// `cd cli && UPDATE_DOCS=1 cargo test --offline --test docs`) and from cli/schema.json (the app's routes).

import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildPage } from './content.mjs'
import { toHtml, toMarkdown } from './render.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const readJson = (relative) => JSON.parse(readFileSync(join(here, relative), 'utf8'))

export const inputs = () => ({ commands: readJson('commands.json'), examples: readJson('examples.json'), routes: readJson('../../cli/schema.json') })

/** What is wrong with a built page: things a reader would trip over. */
export function lint(html, data) {
  const problems = []
  const ids = [...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])
  for (const id of new Set(ids.filter((id, i) => ids.indexOf(id) !== i))) problems.push(`the id "${id}" is used twice`)
  for (const [, target] of html.matchAll(/href="#([^"]*)"/g)) {
    if (!ids.includes(target)) problems.push(`a link points at #${target}, which is not on the page`)
  }
  if (/<script|<iframe|<object|<embed|@import|url\(\s*["']?https?:|<link[^>]+href="https?:|\ssrc="(https?:)?\/\//i.test(html)) problems.push('the page loads something from outside itself')
  if (/\son[a-z]+="/i.test(html)) problems.push('the page has an inline event handler')
  if ((html.match(/<h1[\s>]/g) ?? []).length !== 1) problems.push('the page must have exactly one h1')
  if (!/<html lang="en">/.test(html) || !/<title>[^<]+<\/title>/.test(html)) problems.push('the page has no language or title')
  for (const c of data.commands.commands) {
    if (!html.includes(`id="cmd-${c.command.replace(/ /g, '-')}"`)) problems.push(`the command "${c.command}" is not on the page`)
  }
  for (const r of data.routes.routes) {
    if (!html.includes(`<td><code>${r.name}</code></td>`)) problems.push(`the route "${r.name}" is not in the HTTP API table`)
  }
  for (const code of Object.keys(data.commands.exitCodes)) {
    if (!html.includes(`<td><strong>${code}</strong></td>`)) problems.push(`exit code ${code} is not explained`)
  }
  for (const code of Object.keys(data.commands.errorCodes)) {
    if (!html.includes(`<td><code>${code}</code></td>`)) problems.push(`error code ${code} is not explained`)
  }
  if (html.length > 1_500_000) problems.push('the page is over 1.5 MB')
  return problems
}

export function build(data = inputs()) {
  const page = buildPage(data)
  const html = toHtml(page)
  return { page, html, markdown: toMarkdown(page), css: readFileSync(join(here, 'site.css'), 'utf8'), problems: lint(html, data) }
}

function main() {
  const check = process.argv.includes('--check')
  let built
  try {
    built = build()
  } catch (error) {
    console.error(`docs: ${error.message}`)
    process.exit(1)
  }
  if (built.problems.length) {
    for (const p of built.problems) console.error(`docs: ${p}`)
    process.exit(1)
  }
  const markdownPath = join(here, '..', 'CLI.md')
  if (check) {
    const current = existsSync(markdownPath) ? readFileSync(markdownPath, 'utf8') : ''
    if (current !== built.markdown) {
      console.error('docs: docs/CLI.md is out of date. Run: node docs/site/build.mjs')
      process.exit(1)
    }
    console.log(`docs: ok (${built.page.sections.length} sections, ${built.html.length} bytes of HTML)`)
    return
  }
  const site = join(here, '_site')
  mkdirSync(site, { recursive: true })
  writeFileSync(markdownPath, built.markdown)
  writeFileSync(join(site, 'index.html'), built.html)
  writeFileSync(join(site, 'style.css'), built.css)
  copyFileSync(markdownPath, join(site, 'CLI.md'))
  // The pages are plain files; GitHub must not run them through Jekyll.
  writeFileSync(join(site, '.nojekyll'), '')
  console.log(`docs: wrote docs/CLI.md and docs/site/_site/ (${built.html.length} bytes of HTML)`)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
