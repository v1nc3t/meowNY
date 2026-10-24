#!/usr/bin/env node
// Builds the website for GitHub Pages: the landing page, the guide, the command line reference (made from the program's
// own description of itself, with a Markdown copy, CLI.md, for scripts and AI agents) and the develop page.
//
//   node docs/site/build.mjs            write the site in docs/site/_site/
//   node docs/site/build.mjs --check    build in memory, and fail if a page has a problem
//
// No dependencies. The facts come from docs/site/commands.json and docs/site/examples.json (written by the program's tests,
// `cd cli && UPDATE_DOCS=1 cargo test --offline --test docs`) and from cli/schema.json (the app's routes). The colours,
// font and corners are the app's own: ui/src/styles/tokens.css goes in front of docs/site/site.css.

import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildPage } from './content.mjs'
import { buildDevelop } from './develop.mjs'
import { buildGuide } from './guide.mjs'
import { landingHtml } from './landing.mjs'
import { SHOTS, shotFiles } from './shots.mjs'
import { toHtml, toMarkdown } from './render.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const readJson = (relative) => JSON.parse(readFileSync(join(here, relative), 'utf8'))

export const inputs = () => ({ commands: readJson('commands.json'), examples: readJson('examples.json'), routes: readJson('../../cli/schema.json') })

/** The screenshots whose two files (light and dark) are both in docs/site/images/. */
export const images = () => new Set(Object.keys(SHOTS).filter((name) => shotFiles(name).every((f) => existsSync(join(here, 'images', f)))))

/** Files on the site that are not pages. */
const OTHER_FILES = ['style.css', 'CLI.md', 'favicon.svg']

const idsOf = (html) => [...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])

/** What is wrong with any page of the site: things a reader would trip over. */
export function lintPage(html) {
  const problems = []
  const ids = idsOf(html)
  for (const id of new Set(ids.filter((id, i) => ids.indexOf(id) !== i))) problems.push(`the id "${id}" is used twice`)
  for (const [, target] of html.matchAll(/href="#([^"]*)"/g)) {
    if (!ids.includes(target)) problems.push(`a link points at #${target}, which is not on the page`)
  }
  if (/<script|<iframe|<object|<embed|@import|url\(\s*["']?https?:|<link[^>]+href="https?:|\ssrc="(https?:)?\/\//i.test(html)) problems.push('the page loads something from outside itself')
  for (const [, src] of html.matchAll(/\s(?:src|srcset)="([^"]*)"/g)) {
    if (!/^images\/[a-z-]+\.png$/.test(src)) problems.push(`the page loads ${src}, which is not one of the site's pictures`)
  }
  if (/\son[a-z]+="/i.test(html)) problems.push('the page has an inline event handler')
  if ((html.match(/<h1[\s>]/g) ?? []).length !== 1) problems.push('the page must have exactly one h1')
  if (!/<html lang="en">/.test(html) || !/<title>[^<]+<\/title>/.test(html)) problems.push('the page has no language or title')
  if (html.length > 1_500_000) problems.push('the page is over 1.5 MB')
  return problems
}

/** What is wrong with the command line page: the generic checks, and every command, route and code on it. */
export function lint(html, data) {
  const problems = lintPage(html)
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
  return problems
}

/** Links from one page to another: the page must be on the site, and the place on it must exist. */
export function lintLinks(pages) {
  const problems = []
  for (const [file, html] of Object.entries(pages)) {
    for (const [, href] of html.matchAll(/href="([^"#][^"]*)"/g)) {
      if (/^https?:/.test(href)) continue
      const [target, place] = href.split('#')
      if (!pages[target] && !OTHER_FILES.includes(target)) problems.push(`${file}: a link points at ${href}, which is not on the site`)
      else if (place && pages[target] && !idsOf(pages[target]).includes(place)) problems.push(`${file}: a link points at ${href}, which is not on that page`)
    }
  }
  return problems
}

export function build(data = inputs(), shots = images()) {
  const page = buildPage(data)
  const html = toHtml(page)
  const pages = { 'index.html': landingHtml({ images: shots }), 'guide.html': toHtml(buildGuide(shots)), 'cli.html': html, 'develop.html': toHtml(buildDevelop()) }
  const problems = [
    ...lint(html, data).map((p) => `cli.html: ${p}`),
    ...['index.html', 'guide.html', 'develop.html'].flatMap((file) => lintPage(pages[file]).map((p) => `${file}: ${p}`)),
    ...lintLinks(pages),
  ]
  const css = readFileSync(join(here, '../../ui/src/styles/tokens.css'), 'utf8') + '\n' + readFileSync(join(here, 'site.css'), 'utf8')
  return { page, html, pages, shots, markdown: toMarkdown(page), css, problems }
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
  if (check) {
    console.log(`docs: ok (${Object.keys(built.pages).length} pages, ${built.page.sections.length} sections in the command line reference)`)
    return
  }
  const site = join(here, '_site')
  mkdirSync(site, { recursive: true })
  for (const [file, html] of Object.entries(built.pages)) writeFileSync(join(site, file), html)
  writeFileSync(join(site, 'style.css'), built.css)
  writeFileSync(join(site, 'CLI.md'), built.markdown)
  copyFileSync(join(here, '../../ui/public/favicon.svg'), join(site, 'favicon.svg'))
  if (built.shots.size) {
    mkdirSync(join(site, 'images'), { recursive: true })
    for (const name of built.shots) for (const f of shotFiles(name)) copyFileSync(join(here, 'images', f), join(site, 'images', f))
  }
  // The pages are plain files; GitHub must not run them through Jekyll.
  writeFileSync(join(site, '.nojekyll'), '')
  console.log(`docs: wrote docs/site/_site/ (${[...Object.keys(built.pages), 'CLI.md'].join(', ')})`)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
