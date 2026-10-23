#!/usr/bin/env node
// Builds the website for GitHub Pages, and docs/CLI.md. Three pages: the landing page, the guide and the command line
// reference, which is made from the program's own description of itself.
//
//   node docs/site/build.mjs            write docs/CLI.md and the site in docs/site/_site/
//   node docs/site/build.mjs --check    build in memory, and fail if docs/CLI.md is out of date or a page has a problem
//
// No dependencies. The facts come from docs/site/commands.json and docs/site/examples.json (written by the program's tests,
// `cd cli && UPDATE_DOCS=1 cargo test --offline --test docs`) and from cli/schema.json (the app's routes). The colours,
// font and corners are the app's own: ui/src/styles/tokens.css goes in front of docs/site/site.css.

import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildPage } from './content.mjs'
import { buildGuide } from './guide.mjs'
import { landingHtml, SHOTS } from './landing.mjs'
import { toHtml, toMarkdown } from './render.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const readJson = (relative) => JSON.parse(readFileSync(join(here, relative), 'utf8'))

export const inputs = () => ({ commands: readJson('commands.json'), examples: readJson('examples.json'), routes: readJson('../../cli/schema.json') })

/** The screenshots that exist in docs/site/images/. */
export const images = () => new Set(Object.keys(SHOTS).filter((name) => existsSync(join(here, 'images', `${name}.png`))))

/** Files on the site that are not pages. */
const OTHER_FILES = ['style.css', 'CLI.md']

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
  for (const [, src] of html.matchAll(/\ssrc="([^"]*)"/g)) {
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
  const pages = { 'index.html': landingHtml({ images: shots }), 'guide.html': toHtml(buildGuide()), 'cli.html': html }
  const problems = [
    ...lint(html, data).map((p) => `cli.html: ${p}`),
    ...['index.html', 'guide.html'].flatMap((file) => lintPage(pages[file]).map((p) => `${file}: ${p}`)),
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
  const markdownPath = join(here, '..', 'CLI.md')
  if (check) {
    const current = existsSync(markdownPath) ? readFileSync(markdownPath, 'utf8') : ''
    if (current !== built.markdown) {
      console.error('docs: docs/CLI.md is out of date. Run: node docs/site/build.mjs')
      process.exit(1)
    }
    console.log(`docs: ok (${Object.keys(built.pages).length} pages, ${built.page.sections.length} sections in the command line reference)`)
    return
  }
  const site = join(here, '_site')
  mkdirSync(site, { recursive: true })
  writeFileSync(markdownPath, built.markdown)
  for (const [file, html] of Object.entries(built.pages)) writeFileSync(join(site, file), html)
  writeFileSync(join(site, 'style.css'), built.css)
  copyFileSync(markdownPath, join(site, 'CLI.md'))
  if (built.shots.size) {
    mkdirSync(join(site, 'images'), { recursive: true })
    for (const name of built.shots) copyFileSync(join(here, 'images', `${name}.png`), join(site, 'images', `${name}.png`))
  }
  // The pages are plain files; GitHub must not run them through Jekyll.
  writeFileSync(join(site, '.nojekyll'), '')
  console.log(`docs: wrote docs/CLI.md and docs/site/_site/ (${Object.keys(built.pages).join(', ')})`)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
