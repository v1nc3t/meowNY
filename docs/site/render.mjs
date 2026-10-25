// Turns a page (a list of sections made of blocks) into HTML and into Markdown. One model, two outputs, so the
// command line page and its Markdown copy (CLI.md on the site) say the same thing. No dependencies.
//
// Text may use a little markup: `code`, **bold**, *italic* and [text](link). Everything else is plain text and is
// escaped, so nothing a command prints can become markup.
//
// Every page of the site sits in the same frame as the app: a sidebar with the pages and this page's contents, and
// the page beside it (on a narrow screen, the sidebar is a strip above the page).

import { readFileSync } from 'node:fs'

export const REPO = 'https://github.com/v1nc3t/meowNY'
/** Where people can support the project; the money goes toward the App Store and Google Play. */
export const SPONSOR = 'https://ko-fi.com/v1nc3t'

/** The logo, the app's own favicon (scripts/logo.py draws it), for the sidebar. */
const LOGO = readFileSync(new URL('../../ui/public/favicon.svg', import.meta.url), 'utf8').trim().replace('<svg ', '<svg class="logo" aria-hidden="true" ')

/** The pages of the site, in the order of the sidebar. */
export const PAGES = [
  { file: 'index.html', label: 'home', icon: 'home' },
  { file: 'guide.html', label: 'guide', icon: 'guide' },
  { file: 'cli.html', label: 'command line', icon: 'terminal' },
  { file: 'develop.html', label: 'develop', icon: 'build' },
]

/** The app's line icons (ui/src/components/NavIcon.tsx), and a few for the site. */
const ICONS = {
  home: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
  guide: 'M12 6c-2-1.5-5-2-8-2v14c3 0 6 .5 8 2 2-1.5 5-2 8-2V4c-3 0-6 .5-8 2zM12 6v14',
  terminal: 'M4 5h16v14H4zM7.5 9.5l2.5 2.5-2.5 2.5M12 15h4.5',
  source: 'M9 8l-4 4 4 4M15 8l4 4-4 4',
  budgets: 'M5 19V9M12 19V5M19 19v-7',
  transactions: 'M4 7h16M4 12h16M4 17h16',
  recurring: 'M5 12a7 7 0 0 1 12-5l2 2M19 4v5h-5M19 12a7 7 0 0 1-12 5l-2-2M5 20v-5h5',
  documents: 'M7 3h7l5 5v13H7zM14 3v5h5M10 13h6M10 17h6',
  debts: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM14.6 9.3c-.4-.8-1.4-1.3-2.6-1.3-1.4 0-2.6.7-2.6 1.8 0 2.4 5.2 1.2 5.2 3.6 0 1.1-1.2 1.8-2.6 1.8-1.2 0-2.2-.5-2.6-1.3M12 6.5V8M12 16v1.5',
  sync: 'M4 8h14M14 4l4 4-4 4M20 16H6M10 12l-4 4 4 4',
  export: 'M12 15V4M8 8l4-4 4 4M5 14v5h14v-5',
  lock: 'M6 11h12v9H6zM9 11V8a3 3 0 0 1 6 0v3',
  build: 'M14.5 4.5a4 4 0 0 0-5 5L4 15l5 5 5.5-5.5a4 4 0 0 0 5-5l-2.5 2.5-2.5-.5-.5-2.5z',
}

export const icon = (name) => `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="${ICONS[name]}"/></svg>`

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

/** This page's contents for the sidebar: groups of links, each with an optional title. */
export function tocHtml(groups) {
  return groups
    .map(
      (group) =>
        `<div class="group">${group.title ? `<p class="group-title">${group.href ? `<a href="#${group.href}">${escapeHtml(group.title)}</a>` : escapeHtml(group.title)}</p>` : ''}` +
        (group.links.length ? `<ul class="${group.cls ?? ''}">${group.links.map((l) => `<li><a href="#${l.id}">${escapeHtml(l.text)}</a></li>`).join('')}</ul>` : '') +
        `</div>`,
    )
    .join('')
}

/** A whole page of the site: the app's sidebar, then the page. `file` marks the page you are on. */
export function shell({ file, title, description, toc = [], body, wide = false }) {
  const pages = PAGES.map((p) => `<a class="nav-item" href="${p.file}"${p.file === file ? ' aria-current="page"' : ''}>${icon(p.icon)}<span>${escapeHtml(p.label)}</span></a>`).join('\n')
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
<div class="shell">
<aside class="side">
<a class="brand" href="index.html">${LOGO}<span>meowny</span></a>
<nav class="side-nav" aria-label="Pages">
${pages}
</nav>
${toc.length ? `<nav class="toc" aria-label="On this page">\n${tocHtml(toc)}\n</nav>` : ''}
<div class="side-foot"><a class="nav-item" href="${REPO}">${icon('source')}<span>source</span></a></div>
</aside>
<main id="main"${wide ? ' class="wide"' : ''}>
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
  const body = `<header class="hero" id="top"><h1>${escapeHtml(page.title)}</h1><p class="lead">${inlineHtml(page.lead)}</p></header>
${sections}
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
