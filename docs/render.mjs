// Turns a page (a list of sections made of blocks) into HTML and into Markdown. One model, two outputs, so the
// website and docs/CLI.md say the same thing. No dependencies.
//
// Text may use a little markup: `code`, **bold**, *italic* and [text](link). Everything else is plain text and is
// escaped, so nothing a command prints can become markup.

export const escapeHtml = (text) =>
  String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

const PIECE = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|\[[^\]]+\]\([^)\s]+\))/g

/** Only links to the same page, to other pages of the site and to the web are links; `javascript:` is not. */
export const safeLink = (url) => /^(#|\.{0,2}\/|[A-Za-z0-9._-]+(\/|$)|https?:\/\/)/.test(url) && !/^[A-Za-z][A-Za-z0-9+.-]*:(?!\/\/)/.test(url)

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

export function toHtml(page) {
  const nav = page.nav
    .map(
      (group) =>
        `<div class="group"><p class="group-title">${group.href ? `<a href="#${group.href}">${escapeHtml(group.title)}</a>` : escapeHtml(group.title)}</p>` +
        (group.links.length ? `<ul class="${group.cls ?? ''}">${group.links.map((l) => `<li><a href="#${l.id}">${escapeHtml(l.text)}</a></li>`).join('')}</ul>` : '') +
        `</div>`,
    )
    .join('')
  const sections = page.sections
    .map((s) => `<section class="doc" id="${s.id}"><h2>${escapeHtml(s.title)}${anchor(s.id)}</h2>${s.blocks.map(blockHtml).join('\n')}</section>`)
    .join('\n')
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(page.title)}</title>
<meta name="description" content="${escapeHtml(page.description)}">
<link rel="stylesheet" href="style.css">
</head>
<body>
<a class="skip" href="#main">Skip to the content</a>
<div class="layout">
<nav class="side" aria-label="Contents">
<p class="brand"><a href="#top">${escapeHtml(page.brand)}</a></p>
${nav}
</nav>
<main id="main">
<header class="hero" id="top"><h1>${escapeHtml(page.title)}</h1><p class="lead">${inlineHtml(page.lead)}</p></header>
${sections}
<footer><p>${inlineHtml(page.footer)}</p></footer>
</main>
</div>
</body>
</html>
`
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
  const out = [`# ${page.title}`, page.lead, '*This file is generated from the program by `node docs/build.mjs`. Do not edit it by hand.*']
  for (const s of page.sections) {
    out.push(`## ${s.title}`)
    out.push(...s.blocks.map(blockMd))
  }
  out.push(page.footer)
  return out.join('\n\n') + '\n'
}
