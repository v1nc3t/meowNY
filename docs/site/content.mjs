// What the command line page says. Everything that is a fact about the program (the commands, their flags, the exit
// codes and error codes, the examples and what they print) is read from docs/site/commands.json and
// docs/site/examples.json, which the program's own tests write and check. This file adds a little prose around them.
// It stops with an error if the program has something that the prose does not cover, so the page cannot quietly fall
// behind.

import { REPO } from './render.mjs'

/** What each error code means, in a few words. A test fails if the program has a code that is not here. */
const MEANINGS = {
  usage: 'Wrong flags, or a delete without `--yes`. Try `--help`.',
  no_token: 'No token. Run `meowny login` or set `MEOWNY_TOKEN`.',
  bad_token: 'Not a valid token. Copy it again from the app.',
  token_file_unsafe: 'Others can read the token file. Run `chmod 600` on it.',
  app_unavailable: 'The app is not running, or command line access is off.',
  timeout: 'No answer in time. Raise `--timeout`.',
  bad_answer: 'Something else answered. Check `--port` or the data folder.',
  invalid_input: 'The app refused a value; the message says which.',
  unauthorized: 'The token is unknown, expired or revoked. Make a new one.',
  forbidden: 'The token may not do this (read only?).',
  denied: 'You said no in the app, or did not answer.',
  not_found: 'Not there; maybe deleted.',
  unknown_route: 'The app is older than the program. Update the app.',
  method_not_allowed: 'Not a POST (only when calling the HTTP API by hand).',
  name_exists: 'A category with that name exists.',
  over_budget: 'The category budgets would add up to more than the monthly budget.',
  file_exists: 'The file exists. Choose another name.',
  not_set_up: 'The app has no data yet. Finish its first run.',
  too_large: 'The request is over 1 MiB, or the file over 25 MiB.',
  locked: 'The app is locked. Unlock it in the app.',
  internal: 'Something failed in the app. No details, on purpose.',
}

const NOUN_TITLES = {
  settings: 'Settings',
  categories: 'Categories',
  transactions: 'Transactions',
  budgets: 'Budgets',
  recurring: 'Recurring items',
  stats: 'Statistics',
  documents: 'Documents',
  people: 'Friends',
  ledger: 'Ledger',
  debts: 'Debts',
  splits: 'Shared bills',
  export: 'Export',
  backup: 'Backup',
}

/** A note for a command, only where its summary and flags leave out something you need. */
const EXTRA = {
  'transactions create': ['Without `--client-uuid`, one is made and returned.'],
  schema: ['Works offline, with no token. `--app` asks the running app instead.'],
  'export csv': ['Text that starts like a formula is saved as text, so opening the file runs nothing.'],
  'backup create': ['Restoring is done in the app only.'],
}

/** The app's own descriptions use its argument names (`clientUuid`); a person at the command line types the flag. */
function inFlags(text, command) {
  let out = text
  for (const f of command.flags) {
    if (f.param && /[A-Z]/.test(f.param)) out = out.replace(new RegExp(`\\b${f.param}\\b`, 'g'), `\`${f.flag}\``)
  }
  return out
}

function must(condition, message) {
  if (!condition) throw new Error(message)
}

export function buildPage({ commands: data, examples: recorded, routes }) {
  const { commands, cli } = data
  const examples = recorded.examples

  // ---- the command reference, from the program ----
  const groups = new Map([['general', { title: 'General', words: [], commands: [] }]])
  for (const c of commands) {
    const words = c.command.split(' ')
    const key = words.length === 1 ? 'general' : words[0]
    if (!groups.has(key)) {
      must(NOUN_TITLES[key], `the program has a new group of commands, "${key}", that docs/site/content.mjs does not know`)
      groups.set(key, { title: NOUN_TITLES[key], commands: [] })
    }
    groups.get(key).commands.push(c)
  }
  // The general commands in the order a person meets them.
  const first = ['status', 'today', 'login', 'logout', 'schema']
  groups.get('general').commands.sort((a, b) => first.indexOf(a.command) - first.indexOf(b.command))

  // The badges already say "needs --yes" and "asks you in the app", and the usage line shows a plain id, so no note repeats them.
  const commandBlock = (c) => {
    const example = examples[c.command]
    must(example, `no example for the command "${c.command}"`)
    const flags = c.flags.map((f) => ({
      flag: f.flag,
      required: f.required === true,
      type: f.clears ? 'no value' : f.values?.length ? `one of ${f.values.join(', ')}` : f.value,
      summary: inFlags(f.summary, c),
    }))
    return {
      t: 'command',
      id: `cmd-${c.command.replace(/ /g, '-')}`,
      command: { name: c.command, summary: inFlags(c.summary, c), usage: c.usage, scope: c.scope ?? 'local', confirm: c.confirm, destructive: c.destructive, flags },
      notes: [...(EXTRA[c.command] ?? [])],
      example,
    }
  }
  for (const name of Object.keys(EXTRA)) must(commands.some((c) => c.command === name), `docs/site/content.mjs explains "${name}", which is not a command`)

  const commandSection = {
    id: 'commands',
    title: 'Commands',
    blocks: [{ t: 'p', text: `${commands.length} commands. \`--help\` works everywhere, without the app.` }],
  }
  for (const [key, g] of groups) {
    commandSection.blocks.push({ t: 'h3', id: `group-${key}`, text: g.title })
    for (const c of g.commands) commandSection.blocks.push(commandBlock(c))
  }

  // ---- exit codes and errors ----
  const errorCodes = Object.entries(data.errorCodes)
  for (const [code] of errorCodes) must(MEANINGS[code], `the error code "${code}" has no explanation in docs/site/content.mjs`)
  for (const code of Object.keys(MEANINGS)) must(data.errorCodes[code] !== undefined, `docs/site/content.mjs explains "${code}", which the program does not have`)
  const errorSection = {
    id: 'errors',
    title: 'Exit codes and errors',
    blocks: [
      { t: 'table', head: ['Exit code', 'Means'], rows: Object.entries(data.exitCodes).map(([code, text]) => [`**${code}**`, text]) },
      { t: 'p', text: 'With `--json`, an error is one line on standard error: `{"code": "...", "message": "..."}`.' },
      { t: 'table', head: ['Code', 'Exit', 'Means'], rows: errorCodes.map(([code, exit]) => [`\`${code}\``, String(exit), MEANINGS[code]]) },
    ],
  }

  // ---- reference ----
  const referenceSection = {
    id: 'reference',
    title: 'Global flags and settings',
    blocks: [
      { t: 'h3', id: 'global-flags', text: 'Flags every command takes' },
      { t: 'table', head: ['Flag', 'What it does'], rows: data.globalFlags.map((f) => [`\`${f.flag}\``, f.summary]) },
      { t: 'h3', id: 'environment', text: 'Environment variables' },
      { t: 'table', head: ['Variable', 'What it is'], rows: data.environment.map((e) => [`\`${e.name}\``, e.summary]) },
      { t: 'p', text: 'Token order: `--token-file`, `MEOWNY_TOKEN`, `MEOWNY_TOKEN_FILE`, then the file `meowny login` saved.' },
      { t: 'h3', id: 'where', text: 'Where things are' },
      {
        t: 'table',
        head: ['', 'Linux', 'macOS', 'Windows'],
        rows: [
          ['The app\'s data folder', '`~/.local/share/org.meowny.app/`', '`~/Library/Application Support/org.meowny.app/`', '`%APPDATA%\\org.meowny.app\\`'],
          ['The saved token', '`~/.config/meowny/token`', '`~/Library/Application Support/meowny/token`', '`%APPDATA%\\meowny\\token`'],
        ],
      },
    ],
  }

  // ---- the HTTP API, from the app's own route list ----
  must(Array.isArray(routes.routes) && routes.routes.length > 30, 'cli/schema.json has no routes')
  const httpRows = routes.routes.map((r) => {
    const args = r.params.map((p) => `\`${p.name}\`${p.required ? ' (required)' : ''}`).join(', ')
    return [`\`${r.name}\``, r.confirm ? `${r.scope}, and your yes` : r.scope, args || 'none', r.summary]
  })
  const statusJson = examples.status.json.stdout.trim()
  const httpSection = {
    id: 'http-api',
    title: 'The HTTP API behind it',
    blocks: [
      { t: 'p', text: 'The command line wraps a small HTTP API on `127.0.0.1`. Its port is in `api.json` in the data folder.' },
      {
        t: 'ul',
        items: [
          '`POST /v1/<route>` with a JSON body and `Authorization: Bearer <token>`.',
          'Answers `{"ok": true, "data": ...}` or `{"ok": false, "error": {...}}`.',
          'Requests from web pages (with an `Origin` header) are refused.',
          'Bodies up to 1 MiB; `documents.add` up to 35 MiB.',
        ],
      },
      { t: 'code', label: 'Ask the app for its status', text: `curl -s -X POST "http://127.0.0.1:$PORT/v1/status" \\\n  -H "Authorization: Bearer $MEOWNY_TOKEN"\n\n{"data":${statusJson},"ok":true}` },
      { t: 'table', head: ['Route', 'Token', 'Arguments', 'What it does'], rows: httpRows, cls: 'routes' },
    ],
  }

  // ---- the rest of the prose ----
  const guide = [
    {
      id: 'overview',
      title: 'Overview',
      blocks: [
        { t: 'p', text: '`meowny` talks to the meowNY desktop app running on this computer. It never opens your data file.' },
        { t: 'note', kind: 'info', text: 'The app must be running and unlocked, with **settings › command line access** turned on.' },
      ],
    },
    {
      id: 'quick-start',
      title: 'Quick start',
      blocks: [
        {
          t: 'ol',
          items: [
            'In the app: **settings › command line access**. Turn it on and make a token. It is shown once.',
            'Install: `cargo install --path cli`',
            'Save the token: `echo "$TOKEN" | meowny login`',
          ],
        },
        { t: 'code', text: 'meowny status\nmeowny transactions list --limit 5' },
      ],
    },
    {
      id: 'concepts',
      title: 'How it works',
      blocks: [
        { t: 'h3', id: 'tokens', text: 'Tokens' },
        {
          t: 'ul',
          items: [
            'Made in the app, starting with `mny_`. Treat one like a password.',
            'Read only, or read and write. Lasts 1 to 365 days; revoke it any time.',
            'No `--token` flag: use `meowny login`, `MEOWNY_TOKEN` or `--token-file`.',
          ],
        },
        { t: 'h3', id: 'files', text: 'Files need your yes' },
        {
          t: 'ul',
          items: [
            'Exports, backups and copies of documents wait for you to click **Allow** in the app.',
            'The file must not exist yet: nothing is overwritten.',
            'After a no, the token is refused for 30 seconds.',
          ],
        },
        { t: 'h3', id: 'conventions', text: 'Conventions' },
        {
          t: 'ul',
          items: [
            'App locked: exit code 3. App not running: exit code 1.',
            '`--json` prints one line of JSON. Errors go to standard error.',
            'Money is text (`12.50`), dates `YYYY-MM-DD`, months `YYYY-MM`.',
            'Deleting needs `--yes`. `--clear-<flag>` empties a field.',
            'A flag given `-` reads standard input. Nothing ever prompts.',
          ],
        },
      ],
    },
  ]

  const agents = {
    id: 'agents',
    title: 'For AI agents and scripts',
    blocks: [
      {
        t: 'ol',
        items: [
          'Use a read-only token in `MEOWNY_TOKEN`.',
          'Run `meowny schema` once to learn every command.',
          'Always pass `--json`, and act on the exit code.',
          'Give each new transaction your own `--client-uuid`.',
          'Exports and backups wait for the user. Do not retry after `denied`.',
          'Never print the token.',
        ],
      },
      {
        t: 'code',
        label: 'For the agent\'s instructions',
        text: 'Use the `meowny` command to read and update the user\'s budget (the meowNY app must be running and unlocked).\nThe token is in $MEOWNY_TOKEN. Run `meowny schema` once to learn the commands.\nAlways add --json. Exit codes: 0 ok, 1 error, 2 bad usage, 3 token refused or app locked, 4 not found.\nAmounts are strings like "12.50". Give new transactions your own --client-uuid.\nDelete only when the user asks (--yes). Exports and backups need the user to click Allow; do not retry a denial.\nNever reveal the token.',
      },
      { t: 'p', text: 'This page as Markdown: [CLI.md](CLI.md).' },
    ],
  }

  const security = {
    id: 'security',
    title: 'Security',
    blocks: [
      {
        t: 'ul',
        items: [
          'Off by default, and on `127.0.0.1` only.',
          'Every request needs a token. A locked app gives nothing.',
          'Web pages cannot use it.',
          'Files need your yes, each time.',
          'No restore, wipe or currency change through it.',
        ],
      },
      { t: 'note', kind: 'warn', text: 'A leaked token is a key to your data. Revoke it in **settings › command line access**.' },
    ],
  }

  const sections = [...guide, commandSection, referenceSection, errorSection, agents, httpSection, security]

  // ---- the side navigation ----
  const nav = [
    { title: 'Basics', links: [{ id: 'overview', text: 'Overview' }, { id: 'quick-start', text: 'Quick start' }, { id: 'tokens', text: 'Tokens' }, { id: 'files', text: 'Files need your yes' }, { id: 'conventions', text: 'Conventions' }] },
    { title: 'Commands', href: 'commands', cls: 'commands', links: [] },
    ...[...groups].map(([key, g]) => ({ title: g.title, href: `group-${key}`, cls: 'commands', links: g.commands.map((c) => ({ id: `cmd-${c.command.replace(/ /g, '-')}`, text: key === 'general' ? c.command : c.command.split(' ')[1] })) })),
    { title: 'Reference', links: [{ id: 'reference', text: 'Flags and settings' }, { id: 'errors', text: 'Exit codes and errors' }, { id: 'agents', text: 'For AI agents' }, { id: 'http-api', text: 'The HTTP API' }, { id: 'security', text: 'Security' }] },
  ]

  return {
    file: 'cli.html',
    title: 'meowny command line',
    description: 'Use meowNY from a terminal, a script or an AI agent: commands, flags, examples, exit codes and the HTTP API.',
    lead: 'Use meowNY from a terminal, a script or an AI agent.',
    footer: `Generated from \`meowny\` ${cli.version} (API ${cli.api}); the examples are real output. [Source](${REPO}).`,
    nav,
    sections,
  }
}
