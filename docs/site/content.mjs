// What the page says. Everything that is a fact about the program (the commands, their flags, the exit codes and
// error codes, the examples and what they print) is read from docs/site/commands.json and docs/site/examples.json, which
// the program's own tests write and check. This file adds the prose around them. It stops with an error if the
// program has something that the prose does not cover, so the page cannot quietly fall behind.

const REPO = 'https://github.com/v1nc3t/meowNY'

/** What each error code means and what to do about it. A test fails if the program has a code that is not here. */
const MEANINGS = {
  usage: 'The command line is wrong: a flag is missing or unknown, or a delete has no `--yes`. Nothing was sent. Run the command with `--help`.',
  no_token: 'No token was given. Set `MEOWNY_TOKEN`, use `--token-file`, or save one with `meowny login`.',
  bad_token: 'What was given is not a token. A token is `mny_` and 64 letters and digits. Copy it again from the app.',
  token_file_unsafe: 'The token file can be read by other users of the computer, so it is not used. Run `chmod 600` on it.',
  app_unavailable: 'The app is not running, or command line access is off. Start the app, and turn it on in Settings, command line access.',
  timeout: 'The app did not answer in time (30 seconds, or 3 minutes for a command you have to allow). Raise it with `--timeout`.',
  bad_answer: 'Something answered that is not the app. Check that `--port` or the data folder points at meowNY.',
  invalid_input: 'The app refused a value (an amount with one decimal, a date that does not exist). The message says which.',
  unauthorized: 'The app does not accept the token: it is unknown, expired or revoked. Make a new one in Settings.',
  forbidden: 'The token may not do this, usually because it is read only. Make a token that can write.',
  denied: 'You (or nobody, in time) did not allow it in the app. After a No the same token is refused for 30 seconds.',
  not_found: 'The transaction, category or other record is not there (maybe it was already deleted).',
  unknown_route: 'This app has no such command: the program is newer than the app. Update the app.',
  method_not_allowed: 'The request was not a POST. The program always sends one; you only see this when using the HTTP API by hand.',
  name_exists: 'A category with that name is already there.',
  file_exists: 'The file already exists. A file is never replaced; choose another name.',
  not_set_up: 'The app has no data yet. Open it and finish the first-run screen.',
  too_large: 'The request is too large (over 1 MiB), or the file is over 25 MiB, the most the app keeps.',
  locked: 'The app is locked. Unlock it in the app and try again. The command line cannot unlock it.',
  internal: 'Something went wrong inside the app. Nothing is shown about what, on purpose, because it could quote your data.',
}

const NOUN_TITLES = {
  settings: 'Settings',
  categories: 'Categories',
  transactions: 'Transactions',
  budgets: 'Budgets',
  recurring: 'Recurring items',
  stats: 'Statistics',
  documents: 'Documents',
  export: 'Export',
  backup: 'Backup',
}

/** Extra explanation for a command, beyond what its summary and flags say. */
const EXTRA = {
  'transactions create': [
    'A fresh `--client-uuid` is made if you do not give one, and it is in the answer. If a command fails or times out and you do not know whether the transaction was recorded, run it again with the same `--client-uuid`: the app returns the first transaction instead of making a second.',
    'The category decides whether this is income or an expense.',
  ],
  'transactions update': ['Only the flags you give change. `--clear-description` removes the description.'],
  'budgets set': [
    'Without `--category-id` this is the global budget of that side (income or expense). A month that has already started changes only that month; this month and later ones carry the new amount forward, so past months keep what they showed.',
  ],
  'budgets remove': ['The same month rules as `budgets set`. The change history keeps what the budget was.'],
  'recurring create': ['Leave out `--amount` for a bill that changes every time (electricity, a phone bill). Paying it then needs `--amount`.'],
  'recurring pay': ['A bill with no fixed amount needs `--amount`. The next due date moves on by itself.'],
  'recurring update': ['`--clear-amount` makes it a bill with no fixed amount; `--clear-end-date` and `--clear-description` remove those.'],
  'settings set': ['The currency is not here: changing it can convert every amount, so it is done in the app.'],
  'schema': [
    'Answers from what the program was built with, so it works with no app and no token. With `--app` it asks the running app for its own list of routes instead (needs a token).',
  ],
  'login': [
    'Reads one line from standard input, so the token never appears in a command line (which other users can read). On Linux and macOS the file is written readable only by you, and a token file that other users can read is refused; on Linux it is `~/.config/meowny/token`.',
  ],
  'logout': ['Removes the saved token. A token in `MEOWNY_TOKEN` is not touched.'],
  'documents list': ['Names, dates, types, sizes and hashes of the documents, newest paper first, never the files. `--transaction-id` and `--recurring-id` show what is attached to one thing. A read-only token can do this.'],
  'documents show': ['One document, and the transactions and recurring items it is attached to. Never the file. `has_file` says whether this computer holds the file. A read-only token can do this.'],
  'documents add': [
    'The program reads the file itself, as you, and sends its bytes to the app, which stores it encrypted. The app never opens a path on a token\'s behalf. The name is the name of the file unless you give `--name`. A file of at most 25 MiB; the same file added again is not stored twice: the answer says `alreadyThere` and the attachments are added to the document that is there.',
    '`--transaction-ids` and `--recurring-ids` attach it to what it belongs to, as comma-separated ids. `--keep-on-phone` also keeps the file on the phone after a sync.',
  ],
  'documents update': ['Only the flags you give change. The contents of a document cannot be changed: add the new file and delete the old one.'],
  'documents delete': ['Deletes the document, its file and its attachments. The transactions it was attached to stay.'],
  'documents link': ['Give exactly one of `--transaction-id` and `--recurring-id`. Attaching what is attached already changes nothing.'],
  'documents unlink': ['Detaches it. The document and the transaction stay.'],
  'documents get': [
    'The only command that gives a file back, so it needs a token that can write **and your yes**: the app shows which token, which document, its size and the exact file, and writes it only if you click Allow. The copy is **not encrypted**.',
    'The file name must end in the extension of the kind of file the app found, never one the caller picks: `.pdf`, `.png`, `.jpg`, `.gif`, `.webp`, `.txt`, `.html`, `.csv`, or `.bin` for a file of no known kind. So a copy cannot be given a name a system would run (`.sh`, `.bat`, `.desktop`). A file that exists is never replaced.',
  ],
  'export preview': ['Nothing is written and nothing is asked: this only says how many transactions an export with these choices would contain. Use it to check a selection before exporting.'],
  'export json': ['The whole data as one file, including the change history unless you chose categories. Documents are listed by name, date, type, size, hash and what they are attached to, never their files. It is not encrypted.'],
  'export csv': ['One row per transaction, with a byte order mark so Excel reads the accents, and a last column with the names of the documents attached to it. Text that starts like a formula is written as text, so opening the file cannot run anything.'],
  'export xlsx': ['Sheets for transactions, categories, budgets, recurring items, a monthly summary and the documents (listed, never the files). Dates are real dates and amounts are numbers.'],
  'export pdf': ['A printable report of whole months, in English from the command line (the app writes it in your language). Leave out `--show-descriptions` to share it without private details.'],
  'backup create': [
    'Everything in one encrypted file, except the files of the documents: their names, dates and links are in it, the files are not. A backup made in the app can include them. Give `--passphrase -` to read the passphrase from standard input so it does not show in the list of running programs. Keep it: without it the backup cannot be opened.',
    'Restoring a backup is done in the app, never with a token.',
  ],
  'backup schedule': ['Shows the scheduled backup the app makes (folder, how often, how the last one went). Setting it up is done in the app.'],
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

  const commandBlock = (c) => {
    const example = examples[c.command]
    must(example, `no example for the command "${c.command}"`)
    const notes = [...(EXTRA[c.command] ?? [])]
    if (c.positional) notes.push(`The ${c.positional} can be given as a plain word, as in \`meowny ${c.command} 42\`, or with \`--${c.positional}\`.`)
    if (c.confirm) notes.push('The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).')
    if (c.destructive) notes.push('This cannot be undone, so it needs `--yes`.')
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
      notes,
      example,
    }
  }
  for (const name of Object.keys(EXTRA)) must(commands.some((c) => c.command === name), `docs/site/content.mjs explains "${name}", which is not a command`)

  const overviewRows = commands.map((c) => [`[\`${c.command}\`](#cmd-${c.command.replace(/ /g, '-')})`, inFlags(c.summary, c), c.scope === 'write' ? 'write token' : c.scope === 'read' ? (c.confirm ? 'read token, and your yes' : 'read token') : 'this computer'])

  const commandSection = {
    id: 'commands',
    title: 'Commands',
    blocks: [
      { t: 'p', text: `Every command is \`meowny <noun> <verb>\`, or a single word, with its flags after it. \`--help\` works at every level, with no app running. There are ${commands.length} commands; each is listed here with what it prints.` },
      { t: 'table', head: ['Command', 'What it does', 'Needs'], rows: overviewRows },
    ],
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
      { t: 'p', text: 'A command ends with one of these exit codes, so a script can tell what happened without reading any text.' },
      { t: 'table', head: ['Exit code', 'Means'], rows: Object.entries(data.exitCodes).map(([code, text]) => [`**${code}**`, text]) },
      { t: 'p', text: 'With `--json`, an error is one line on standard error, `{"code": "...", "message": "..."}`. The `code` is one of these and does not change; the message is for people.' },
      { t: 'table', head: ['Code', 'Exit', 'What it means and what to do'], rows: errorCodes.map(([code, exit]) => [`\`${code}\``, String(exit), MEANINGS[code]]) },
      { t: 'p', text: 'What failing really looks like, each recorded from the program:' },
      ...recorded.errors.map((e) => ({ t: 'code', label: `${e.title} (exit code ${e.exit})`, text: `$ ${e.command}\n${e.stderr.replace(/\n$/, '')}` })),
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
      { t: 'p', text: 'The token is looked for in this order, and the first one found is used: `--token-file`, then `MEOWNY_TOKEN`, then `MEOWNY_TOKEN_FILE`, then the file `meowny login` saved. A token file that other users can read is refused.' },
      { t: 'h3', id: 'where', text: 'Where things are' },
      {
        t: 'table',
        head: ['', 'Linux', 'macOS', 'Windows'],
        rows: [
          ['The app\'s data folder (it writes its port there)', '`~/.local/share/org.meowny.app/`', '`~/Library/Application Support/org.meowny.app/`', '`%APPDATA%\\org.meowny.app\\`'],
          ['The token `meowny login` saves', '`~/.config/meowny/token`', '`~/Library/Application Support/meowny/token`', '`%APPDATA%\\meowny\\token`'],
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
      { t: 'p', text: 'The command line is a thin client. Anything it does you can do yourself with `curl`, because the app serves a small HTTP API on this computer only (`127.0.0.1`, a port the app picks and writes to `api.json` in its data folder). It is off until you turn it on.' },
      {
        t: 'ul',
        items: [
          'Every call is `POST /v1/<route>` with a JSON object as the body (nothing means `{}`) and the header `Authorization: Bearer <token>`.',
          'The answer is JSON: `{"ok": true, "data": ...}`, or `{"ok": false, "error": {"code": "...", "message": "..."}}` with a matching HTTP status (400, 401, 403, 404, 409, 413, 423, 500).',
          'The `Host` header must be `127.0.0.1:<port>` or `localhost:<port>`, and a request with an `Origin` header is refused, so a web page in your browser cannot use it.',
          'Money is text with two decimals (`"12.50"`), ids are text, dates are `YYYY-MM-DD`, months `YYYY-MM`. Unknown or mistyped arguments are a 400 that names them.',
          'Routes marked "and your yes" wait (up to two minutes) for you to allow them in the app, and answer `denied` if you do not.',
          'A body is at most 1 MiB, except for `documents.add`: it takes the file as base64url text in the argument `data` (the command line does this for you), so its body may be up to 35 MiB, and is read only after the token and its scope have been checked from the request\'s head.',
        ],
      },
      { t: 'code', label: 'Ask the app for its status', text: `curl -s -X POST "http://127.0.0.1:$PORT/v1/status" \\\n  -H "Authorization: Bearer $MEOWNY_TOKEN"\n\n{"data":${statusJson},"ok":true}` },
      { t: 'p', text: `The port is in \`api.json\`. \`meowny schema --app\` prints every route with its arguments as JSON.` },
      { t: 'table', head: ['Route', 'Token', 'Arguments', 'What it does'], rows: httpRows, cls: 'routes' },
    ],
  }

  // ---- the rest of the prose ----
  const guide = [
    {
      id: 'overview',
      title: 'Overview',
      blocks: [
        { t: 'p', text: '`meowny` is a small program that drives the meowNY desktop app, for scripts, for AI agents and for anyone who prefers a terminal. It never opens your data file. It asks the running app, over a connection only this computer can reach, and the app does the work with the same rules as its own screens: the same amounts, the same budgets, the same checks.' },
        { t: 'ul', items: ['**Look:** list transactions, budgets and recurring items; see budget against actual, the year, the top categories.', '**Change:** record a transaction, set a budget, pay a recurring bill.', '**Documents:** add a receipt or a contract from a file, attach it to a transaction, list what is there and, with your yes, get a copy back.', '**Take data out:** export to JSON, CSV, Excel or PDF, or make an encrypted backup, each only when you allow it in the app.'] },
        { t: 'note', kind: 'info', text: 'The app has to be running and unlocked, and command line access has to be turned on in its Settings. It is off by default. The phone app has no command line.' },
      ],
    },
    {
      id: 'quick-start',
      title: 'Quick start',
      blocks: [
        { t: 'p', text: '**1. Turn it on.** In the app, open Settings, find *command line access* and tick *allow the command line to connect*.' },
        { t: 'p', text: '**2. Make a token.** In the same block choose a name, read only or read and write, and how long it lasts, then *make token*. Copy it now: it is shown once, and the app keeps only a fingerprint of it.' },
        { t: 'p', text: '**3. Install the program.** For now, build it from the source folder (you need Rust):' },
        { t: 'code', text: 'cargo install --path cli' },
        { t: 'p', text: '**4. Give it the token,** in the environment, or save it once:' },
        { t: 'code', text: 'export MEOWNY_TOKEN=mny_...\n\n# or, once, from standard input (never put it on a command line):\necho "$TOKEN" | meowny login' },
        { t: 'p', text: '**5. Try it.**' },
        { t: 'code', text: 'meowny status\nmeowny transactions list --limit 5\nmeowny stats budget --month 2026-10' },
        { t: 'note', kind: 'info', text: 'Start with a read-only token. A token that can write can add, change and delete your data until it expires or you revoke it in Settings.' },
      ],
    },
    {
      id: 'concepts',
      title: 'How it works',
      blocks: [
        { t: 'h3', id: 'tokens', text: 'Tokens' },
        { t: 'p', text: 'A token is a long secret that starts with `mny_`. You make it in the app. It is **read only** unless you choose read and write (which also reads), it lasts 1 to 365 days (30 by default), and you can revoke it at any time; it stops working at once. The app stores only a fingerprint of it, so nobody can read tokens back from your data file.' },
        { t: 'p', text: 'There is deliberately no `--token` flag: a command line can be read by other users of the computer. Use `MEOWNY_TOKEN`, `--token-file`, or `meowny login`.' },
        { t: 'h3', id: 'files', text: 'Files need your yes' },
        { t: 'p', text: 'An export, a backup or a copy of a document writes a file on your computer, so a token alone is not enough. Each request shows a dialog in the app: which token is asking, what it will write, the exact file, and what is in it (how many transactions, or which document). Nothing is written until you click Allow.' },
        { t: 'ul', items: ['The file must have the right ending (`.json`, `.csv`, `.xlsx`, `.pdf`, `.meowbak`, or for a document the extension of its kind), be in a folder that exists, and **not exist yet**. A file is never replaced.', 'A relative `--out` is made absolute from where you run the program, and the dialog shows the full path.', 'One question at a time. After a No, the same token is refused for 30 seconds, so a script cannot wear you down. No answer in two minutes counts as No.', 'On Linux and macOS the files are readable only by you. They are not encrypted, except a backup, which is locked with the passphrase you give.', 'Restoring a backup, changing the currency, the lock settings, tokens, setting up a scheduled backup and deleting everything cannot be done through the command line at all. They are done in the app.'] },
        { t: 'h3', id: 'locked', text: 'A locked app, a closed app' },
        { t: 'p', text: 'If the app is locked (it locks itself after a while, or when you minimise it, as you set it up), every command answers `locked` (exit code 3) and prints no data. The command line cannot unlock it, and using it does not keep the app awake. If the app is not running, or command line access is off, you get `app_unavailable` (exit code 1).' },
        { t: 'h3', id: 'output', text: 'What is printed' },
        { t: 'p', text: 'Without `--json` you get a plain table or `key: value` lines, and messages (such as "done.") on standard error. With `--json` the data is one line of JSON on standard output, exactly as the app sent it, and an error is a JSON line on standard error. Data is always on standard output and messages on standard error, so `meowny ... | jq` works. A page of a longer list says on standard error where the next page starts.' },
        { t: 'h3', id: 'conventions', text: 'Conventions' },
        { t: 'ul', items: ['**Money** is text with two decimals, like `12.50`. It is never a floating-point number anywhere, so nothing is rounded on the way.', '**Ids** are text, as the app lists them. **Dates** are `YYYY-MM-DD`, **months** `YYYY-MM`, in the time zone set in the app.', '**Words** such as `expense` or `INCOME` can be written in any case.', '**Deleting** (`delete`, `remove`, `end`) needs `--yes`.', '**Clearing** a field uses `--clear-<flag>`; leaving a flag out leaves the field as it is.', 'A text flag given `-` reads its value from standard input.', 'The program never prompts and never waits for input you did not ask it to read (`login` reads the token from standard input, and a text flag given `-` reads its value there).'] },
      ],
    },
  ]

  const agents = {
    id: 'agents',
    title: 'For AI agents and scripts',
    blocks: [
      { t: 'p', text: 'Everything above holds for a script, and a few things make an agent\'s life easy.' },
      {
        t: 'ol',
        items: [
          'Give it a **read-only token** in `MEOWNY_TOKEN`, and a read-and-write token only if it must change things. Tokens expire; ask for a new one when you get exit code 3 with `unauthorized`.',
          'Run `meowny schema` once. It prints, offline and as JSON, every command with its flags, types, whether it needs a token that can write, whether it needs `--yes`, and every exit code and error code.',
          'Always pass `--json`. Read data from standard output; read errors, as `{code, message}`, from standard error. Branch on the **exit code** (0 done, 1 error, 2 bad usage, 3 token or locked, 4 not found).',
          'Create a transaction with a fixed `--client-uuid` of your own. If anything goes wrong, run the same command again: it cannot record the transaction twice.',
          'Exports, backups and copies of documents wait for the person to allow them in the app. Tell the user to look at the app, and do not retry in a loop: after a No the token is refused for 30 seconds. Exit code 3 with `denied` means no.',
          'Never print the token, and do not put it in a command line, a file in a shared folder or a chat.',
        ],
      },
      {
        t: 'code',
        label: 'A paragraph to paste into the agent\'s instructions',
        text: 'You can read and update the user\'s budget with the `meowny` command (the meowNY desktop app must be running and unlocked).\nThe token is in $MEOWNY_TOKEN. Run `meowny schema` once to learn the commands and flags.\nAlways add --json. Data is on stdout; errors are JSON {code, message} on stderr; the exit code is 0 ok,\n1 error, 2 bad usage, 3 token refused or app locked, 4 not found. Amounts are strings like "12.50".\nCreate transactions with your own --client-uuid so a retry cannot duplicate them. Deleting needs --yes: only\ndo it when the user asked. Exports, backups and copies of documents need the user to click Allow in the app: ask them to, and\ndo not retry after a denial. Never reveal the token.',
      },
      { t: 'p', text: 'To read the same facts from a file rather than a terminal, this page is also published as plain Markdown: [CLI.md](CLI.md).' },
    ],
  }

  const security = {
    id: 'security',
    title: 'Security',
    blocks: [
      { t: 'ul', items: [
        '**Off by default.** Nothing listens until you turn command line access on in the app.',
        '**This computer only.** The app listens on `127.0.0.1`, never on the network, on a port the system picks.',
        '**A token for every request,** checked against fingerprints kept in your encrypted data. A locked app cannot check any token, so it answers "locked" and gives nothing.',
        '**Web pages cannot use it.** A request that carries an `Origin` header, or the wrong `Host`, is refused, and the API sends no cross-origin permissions.',
        '**Least power by default.** Tokens are read only unless you choose otherwise, expire, and can be revoked.',
        '**Your yes for files,** each time, with the exact file shown, and never over an existing file.',
        '**No restore, no wipe, no currency change** through it, whatever the token.',
        '**It does not keep the app awake.** Calls do not count as activity, so the idle lock still works.',
        '**Nothing leaves the computer.** The command line only ever connects to the app on this computer. There is no telemetry.',
      ] },
      { t: 'note', kind: 'warn', text: 'A leaked read-only token lets someone who can reach this computer read all your data until it expires; a leaked read-and-write token lets them change it too. Revoke a token in Settings, command line access, as soon as you suspect it. The saved token file is protected only by file permissions.' },
      { t: 'p', text: `To report a problem, open an issue at [the project page](${REPO}).` },
    ],
  }

  const sections = [...guide, commandSection, referenceSection, errorSection, agents, httpSection, security]

  // ---- the side navigation ----
  const nav = [
    { title: 'Guide', links: [{ id: 'overview', text: 'Overview' }, { id: 'quick-start', text: 'Quick start' }, { id: 'concepts', text: 'How it works' }, { id: 'tokens', text: 'Tokens' }, { id: 'files', text: 'Files need your yes' }, { id: 'conventions', text: 'Conventions' }] },
    { title: 'Commands', href: 'commands', cls: 'commands', links: [] },
    ...[...groups].map(([key, g]) => ({ title: g.title, href: `group-${key}`, cls: 'commands', links: g.commands.map((c) => ({ id: `cmd-${c.command.replace(/ /g, '-')}`, text: key === 'general' ? c.command : c.command.split(' ')[1] })) })),
    { title: 'Reference', links: [{ id: 'reference', text: 'Flags and settings' }, { id: 'errors', text: 'Exit codes and errors' }, { id: 'agents', text: 'For AI agents' }, { id: 'http-api', text: 'The HTTP API' }, { id: 'security', text: 'Security' }] },
  ]

  return {
    brand: 'meowny',
    title: 'meowny command line',
    description: 'Use your meowNY budget from a terminal, a script or an AI agent. Commands, flags, examples, exit codes and the HTTP API behind it.',
    lead: 'Use your meowNY budget from a terminal, a script or an AI agent. The command line talks to the desktop app on the same computer; your data never leaves it.',
    footer: `meowNY is free software (AGPL-3.0). Source: [${REPO.replace('https://', '')}](${REPO}). This page was generated from \`meowny\` ${cli.version} (API ${cli.api}); its examples are real output.`,
    nav,
    sections,
  }
}
