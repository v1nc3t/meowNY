// Turns a page (a list of sections made of blocks) into HTML and into Markdown. One model, two outputs, so the
// command line page and its Markdown copy (CLI.md on the site) say the same thing. No dependencies.
//
// Text may use a little markup: `code`, **bold**, *italic* and [text](link). Everything else is plain text and is
// escaped, so nothing a command prints can become markup.
//
// Every page of the site has the same frame: a header with the name and the four pages, and one column of text. A long page
// also has its contents: a sticky list beside the text on a wide screen, and a folded list (`<details>`, no script) above
// the text on a narrow one.

import { readFileSync } from 'node:fs'

export const REPO = 'https://github.com/v1nc3t/meowNY'
/** Where people can support the project; the money goes toward the App Store and Google Play. */
export const SPONSOR = 'https://ko-fi.com/v1nc3t'

/** The logo, the app's own favicon (scripts/logo.py draws it), for the header. */
const LOGO = readFileSync(new URL('../../ui/public/favicon.svg', import.meta.url), 'utf8').trim().replace('<svg ', '<svg class="logo" aria-hidden="true" ')

/** The pages of the site, in the order of the header. */
export const PAGES = [
  { file: 'index.html', label: 'home' },
  { file: 'guide.html', label: 'guide' },
  { file: 'cli.html', label: 'command line' },
  { file: 'develop.html', label: 'develop' },
]

export const escapeHtml = (text) =>
  String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

const PIECE = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|\[[^\]]+\]\([^)\s]+\))/g

/** Only links to the same page, to other pages of the site and to the web are links; `javascript:` is not. */
export const safeLink = (url) => /^(#|\.{0,2}\/|[A-Za-z0-9._-]+(\/|#|$)|https?:\/\/)/.test(url) && !/^[A-Za-z][A-Za-z0-9+.-]*:(?!\/\/)/.test(url)

export function inlineHtml(text) {
  return String(text)
    .split(PIECE)
    .map((piece) => {
      if (piece.startsWith('`') && piece.endsWith('`') && piece.length > 2) return `<code>${escapeHtml(piece.slice(1, -1))}</code>`
      if (piece.startsWith('**') && piece.endsWith('**') && piece.length > 4) return `<strong>${escapeHtml(piece.slice(2, -2))}</strong>`
      if (piece.startsWith('*') && piece.endsWith('*') && piece.length > 2) return `<em>${escapeHtml(piece.slice(1, -1))}</em>`
      const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(piece)
      if (link) return safeLink(link[2]) ? `<a href="${escapeHtml(link[2])}">${inlineHtml(link[1])}</a>` : escapeHtml(link[1])
      return escapeHtml(piece)
    })
    .join('')
}

/** Text with the markup left as Markdown, except that a pipe would end a table cell. */
const inlineMd = (text) => String(text).replace(/\|/g, '\\|')

/** JSON for reading: indented. The program itself prints it on one line, and the label says so. */
const prettyJson = (text) => {
  try {
    return JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    return text
  }
}

const badge = (kind, text) => `<span class="badge ${kind}">${escapeHtml(text)}</span>`

function commandBadges(c) {
  const out = [badge(c.scope === 'write' ? 'write' : c.scope === 'read' ? 'read' : 'local', c.scope === 'write' ? 'write token' : c.scope === 'read' ? 'read token' : 'this computer')]
  if (c.confirm) out.push(badge('ask', 'asks you in the app'))
  if (c.destructive) out.push(badge('danger', 'needs --yes'))
  return out
}

function exampleHtml(example) {
  const panels = []
  const show = (label, run, indent = false) => {
    const text = [indent ? prettyJson(run.stdout) : run.stdout, run.stderr].filter(Boolean).join(run.stdout && run.stderr ? '\n' : '')
    const exit = run.exit === 0 ? '' : `\n(exit code ${run.exit})`
    panels.push(`<figure class="code"><figcaption>${escapeHtml(label)}</figcaption><pre><code>${escapeHtml((text || '(no output)').replace(/\n$/, '') + exit)}</code></pre></figure>`)
  }
  if (example.human) show('what you see', example.human)
  if (example.json) show('with --json (one line; shown indented)', example.json, true)
  return `<div class="example"><figure class="code command"><figcaption>run</figcaption><pre><code>${escapeHtml('$ ' + example.command)}</code></pre></figure>${panels.join('')}</div>`
}

function blockHtml(block) {
  switch (block.t) {
    case 'p':
      return `<p>${inlineHtml(block.text)}</p>`
    case 'ul':
      return `<ul>${block.items.map((i) => `<li>${inlineHtml(i)}</li>`).join('')}</ul>`
    case 'ol':
      return `<ol>${block.items.map((i) => `<li>${inlineHtml(i)}</li>`).join('')}</ol>`
    case 'note':
      return `<aside class="note ${block.kind ?? 'info'}"><p>${inlineHtml(block.text)}</p></aside>`
    case 'code':
      return `<figure class="code">${block.label ? `<figcaption>${escapeHtml(block.label)}</figcaption>` : ''}<pre><code>${escapeHtml(block.text)}</code></pre></figure>`
    case 'table':
      return (
        `<div class="table-wrap"><table${block.cls ? ` class="${block.cls}"` : ''}><thead><tr>` +
        block.head.map((h) => `<th scope="col">${escapeHtml(h)}</th>`).join('') +
        `</tr></thead><tbody>` +
        block.rows.map((r) => `<tr>${r.map((cell) => `<td>${inlineHtml(cell)}</td>`).join('')}</tr>`).join('') +
        `</tbody></table></div>`
      )
    // Screenshots made by shots.mjs (gallery), already escaped.
    case 'figures':
      return block.html
    case 'h3':
      return `<h3 id="${block.id}">${escapeHtml(block.text)}${anchor(block.id)}</h3>`
    case 'command': {
      const c = block.command
      const flags = c.flags.length
        ? `<div class="table-wrap"><table class="flags"><thead><tr><th scope="col">Flag</th><th scope="col">Type</th><th scope="col">Description</th></tr></thead><tbody>` +
          c.flags.map((f) => `<tr><td><code>${escapeHtml(f.flag)}</code>${f.required ? ' <span class="req">required</span>' : ''}</td><td>${escapeHtml(f.type)}</td><td>${inlineHtml(f.summary)}</td></tr>`).join('') +
          `</tbody></table></div>`
        : ''
      return (
        `<section class="command" id="${block.id}"><header><h4><code>${escapeHtml(c.name)}</code>${anchor(block.id)}</h4><p class="badges">${commandBadges(c).join(' ')}</p></header>` +
        `<p>${inlineHtml(c.summary)}</p>` +
        `<figure class="code usage"><figcaption>usage</figcaption><pre><code>${escapeHtml(c.usage)}</code></pre></figure>` +
        flags +
        block.notes.map((n) => `<p class="small">${inlineHtml(n)}</p>`).join('') +
        (block.example ? exampleHtml(block.example) : '') +
        `</section>`
      )
    }
    default:
      throw new Error(`unknown block ${block.t}`)
  }
}

const anchor = (id) => ` <a class="anchor" href="#${id}" aria-label="link to this section">#</a>`

/** This page's contents: groups of links, each with an optional title (shown only when there is more than one group). */
export function tocHtml(groups) {
  return groups
    .map(
      (group) =>
        `<div class="group">${group.title && groups.length > 1 ? `<p class="group-title">${group.href ? `<a href="#${group.href}">${escapeHtml(group.title)}</a>` : escapeHtml(group.title)}</p>` : ''}` +
        (group.links.length ? `<ul class="${group.cls ?? ''}">${group.links.map((l) => `<li><a href="#${l.id}">${escapeHtml(l.text)}</a></li>`).join('')}</ul>` : '') +
        `</div>`,
    )
    .join('')
}

/** A whole page of the site: the header, then the contents (wide screens only) and the page. `file` marks the page you are on. */
export function shell({ file, title, description, toc = [], body, wide = false }) {
  const pages = PAGES.map((p) => `<a class="nav-item" href="${p.file}"${p.file === file ? ' aria-current="page"' : ''}>${escapeHtml(p.label)}</a>`).join('\n')
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>${escapeHtml(title)}</title>
<meta name="description" content="${escapeHtml(description)}">
<link rel="icon" type="image/svg+xml" href="favicon.svg">
<link rel="stylesheet" href="style.css">
</head>
<body>
<a class="skip" href="#main">Skip to the content</a>
<header class="top">
<a class="brand" href="index.html">${LOGO}<span>meowny</span></a>
<nav aria-label="Pages">
${pages}
</nav>
</header>
<div class="page${wide ? ' wide' : ''}">
${toc.length ? `<nav class="toc" aria-label="On this page">\n${tocHtml(toc)}\n</nav>` : ''}
<main id="main">
${body}
</main>
</div>
</body>
</html>
`
}

export function toHtml(page) {
  const sections = page.sections
    .map((s) => `<section class="doc" id="${s.id}"><h2>${escapeHtml(s.title)}${anchor(s.id)}</h2>${s.blocks.map(blockHtml).join('\n')}</section>`)
    .join('\n')
  const fold = page.nav.length ? `<details class="fold"><summary>on this page</summary>\n${tocHtml(page.nav)}\n</details>\n` : ''
  const body = `<header class="hero" id="top"><h1>${escapeHtml(page.title)}</h1><p class="lead">${inlineHtml(page.lead)}</p></header>
${fold}${sections}
<footer><p>${inlineHtml(page.footer)}</p></footer>`
  return shell({ file: page.file, title: page.title, description: page.description, toc: page.nav, body })
}

// ---- Markdown -------------------------------------------------------------------------------

function runText(run, indent = false) {
  const text = [indent ? prettyJson(run.stdout) : run.stdout, run.stderr].filter(Boolean).join(run.stdout && run.stderr ? '\n' : '')
  return (text || '(no output)').replace(/\n$/, '') + (run.exit === 0 ? '' : `\n(exit code ${run.exit})`)
}

const fence = (text, lang = '') => '```' + lang + '\n' + text + '\n```'

function blockMd(block) {
  switch (block.t) {
    case 'p':
      return block.text
    case 'ul':
      return block.items.map((i) => `- ${i}`).join('\n')
    case 'ol':
      return block.items.map((i, n) => `${n + 1}. ${i}`).join('\n')
    case 'note':
      return `> **${block.kind === 'warn' ? 'Careful' : 'Note'}:** ${block.text}`
    case 'code':
      return (block.label ? `*${block.label}*\n\n` : '') + fence(block.text)
    case 'table':
      return [`| ${block.head.join(' | ')} |`, `| ${block.head.map(() => '---').join(' | ')} |`, ...block.rows.map((r) => `| ${r.map(inlineMd).join(' | ')} |`)].join('\n')
    case 'figures':
      return ''
    case 'h3':
      return `### ${block.text}`
    case 'command': {
      const c = block.command
      const badges = commandBadges(c).map((b) => b.replace(/<[^>]+>/g, ''))
      const parts = [`#### \`${c.name}\``, `*${badges.join(' · ')}*`, c.summary, fence(c.usage, 'text')]
      if (c.flags.length) {
        parts.push(['| Flag | Type | Description |', '| --- | --- | --- |', ...c.flags.map((f) => `| \`${f.flag}\`${f.required ? ' (required)' : ''} | ${f.type} | ${inlineMd(f.summary)} |`)].join('\n'))
      }
      parts.push(...block.notes)
      if (block.example) {
        parts.push(fence(`$ ${block.example.command}\n\n${[block.example.human && runText(block.example.human)].filter(Boolean).join('')}`.trimEnd(), 'console'))
        if (block.example.json) parts.push('With `--json` (one line; shown indented):\n\n' + fence(runText(block.example.json, true), 'json'))
      }
      return parts.join('\n\n')
    }
    default:
      throw new Error(`unknown block ${block.t}`)
  }
}

export function toMarkdown(page) {
  const out = [`# ${page.title}`, page.lead]
  for (const s of page.sections) {
    out.push(`## ${s.title}`)
    out.push(...s.blocks.map(blockMd))
  }
  out.push(page.footer)
  return out.join('\n\n') + '\n'
}
