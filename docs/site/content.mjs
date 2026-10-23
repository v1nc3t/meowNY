// What the page says. Everything that is a fact about the program (the commands, their flags, the exit codes and
// error codes, the examples and what they print) is read from docs/site/commands.json and docs/site/examples.json, which
// the program's own tests write and check. This file adds the prose around them. It stops with an error if the
// program has something that the prose does not cover, so the page cannot quietly fall behind.

import { REPO } from './render.mjs'

/** What each error code means and what to do about it. A test fails if the program has a code that is not here. */
const MEANINGS = {
  usage: 'Wrong command line: a flag is missing or unknown, or a delete has no `--yes`. Nothing was sent. Try `--help`.',
  no_token: 'No token. Set `MEOWNY_TOKEN`, use `--token-file`, or run `meowny login`.',
  bad_token: 'Not a token. A token is `mny_` and 64 letters and digits. Copy it again from the app.',
  token_file_unsafe: 'Other users can read the token file, so it is not used. Run `chmod 600` on it.',
  app_unavailable: 'The app is not running, or command line access is off. Start it and turn it on in Settings.',
  timeout: 'No answer in time (30 seconds; 3 minutes when you must allow it). Raise `--timeout`.',
  bad_answer: 'Something that is not the app answered. Check `--port` or the data folder.',
  invalid_input: 'The app refused a value, such as an amount with one decimal or a date that does not exist. The message says which.',
  unauthorized: 'The token is unknown, expired or revoked. Make a new one in Settings.',
  forbidden: 'The token may not do this, usually because it is read only. Make one that can write.',
  denied: 'You did not allow it in the app, or did not answer in time. After a No the token is refused for 30 seconds.',
  not_found: 'The record is not there; maybe it was deleted.',
  unknown_route: 'The app has no such command: the program is newer than the app. Update the app.',
  method_not_allowed: 'The request was not a POST. Only seen when calling the HTTP API by hand.',
  name_exists: 'A category with that name already exists.',
  file_exists: 'The file exists. A file is never replaced; choose another name.',
  not_set_up: 'The app has no data yet. Finish its first-run screen.',
  too_large: 'The request is over 1 MiB, or the file is over 25 MiB.',
  locked: 'The app is locked. Unlock it in the app; the command line cannot.',
  internal: 'Something failed inside the app. Nothing more is shown, on purpose: it could quote your data.',
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

/** Extra explanation for a command, beyond what its summary and flags say. */
const EXTRA = {
  'transactions create': [
    'Without `--client-uuid` one is made and returned. After a failure or timeout, run it again with the same `--client-uuid`: the app returns the first transaction instead of making a second.',
    'The category decides income or expense.',
  ],
  'budgets set': ['Without `--category-id` this is the global budget of that side. A past month changes only itself; this month and later ones carry the new amount on.'],
  'budgets remove': ['Same month rules as `budgets set`.'],
  'recurring create': ['Leave out `--amount` for a bill that changes each time; paying it then needs `--amount`.'],
  'recurring pay': ['A bill with no fixed amount needs `--amount`.'],
  'settings set': ['The currency is set in the app only: changing it can convert every amount.'],
  'schema': ['Works offline with no token. `--app` asks the running app for its routes instead (needs a token).'],
  'login': ['Reads the token from standard input, so it never shows in a command line. It is saved readable only by you (`~/.config/meowny/token` on Linux); a token file others can read is refused.'],
  'logout': ['A token in `MEOWNY_TOKEN` is not touched.'],
  'documents list': ['Details only, newest first, never the files.'],
  'documents show': ['Includes what it is attached to, never the file. `has_file` says whether this computer holds it.'],
  'documents add': [
    'The program reads the file as you and sends its bytes to the app, which stores them encrypted. At most 25 MiB. The same file twice is stored once: the answer says `alreadyThere` and the new attachments go to the document that is there.',
  ],
  'documents update': ['The contents cannot change: add the new file and delete the old one.'],
  'documents delete': ['Removes the document, its file and its attachments. The transactions stay.'],
  'documents link': ['Give exactly one of `--transaction-id`, `--recurring-id` and `--ledger-entry-id`.'],
  'documents get': [
    'The only command that gives a file back. It needs a write token **and your yes**: the app shows the token, the document, its size and the exact file. The copy is **not encrypted**.',
    'The file name must end in the extension of the kind of file the app found (`.pdf`, `.png`, `.jpg`, `.gif`, `.webp`, `.txt`, `.html`, `.csv`, or `.bin`), so a copy never gets a name a system would run. An existing file is never replaced.',
  ],
  'people list': ['Largest balance first. A friend is a name only: no phone, email or address. Below zero means you owe them.'],
  'people show': ['Their parts of shared bills and your entries, by date, with the balance after each line.'],
  'people create': ['The name is 1 to 50 characters, unique among active friends, ignoring case.'],
  'people update': ['The reminder flags are kept for old data and do nothing: reminders are per device now (see `debts reminders`).'],
  'people archive': ['Balance and history stay, and the name can be reused. A friend is never deleted.'],
  'people unarchive': ['Fails if another friend has taken the name.'],
  'people settle': [
    'Without `--amount` it is the whole balance. More than the debt is accepted: the balance turns around and `over` in the answer says by how much. Repayments are never income or expense.',
  ],
  'debts summary': ['Neither side is income or expense, and neither touches a budget.'],
  'debts due': ['One line per missed occurrence (rent unpaid for three months is three), oldest first, at most 24 per item. Overdue from the day after its day. Income and paused or ended items are not listed. Pay with `recurring pay`.'],
  'debts history': ['Newest first, paged (`--limit` up to 200, `--offset`). Includes recurring payments paid **late** or **overdue**; on-time and skipped ones leave no line. `--person-id` keeps one friend (no recurring payments).'],
  'debts reminders': ['A friend with a balance is reminded `--every-days` after the debt last changed, then again after as many; 0 is none. This only reads: no notification is sent from the command line.'],
  'ledger add': [
    'Money lent, borrowed or paid back; never income or expense, and in no budget or statistic. `--kind` is `lent`, `borrowed`, `repaid_to_me` or `repaid_by_me`.',
    'For a borrowing, `--expense-name` and `--expense-category-id` add your share to your expenses in the same step. Paying back later is another entry, never a second expense.',
  ],
  'ledger update': ['The expense an entry came with is not changed.'],
  'ledger delete': ['The friend\'s balance changes by that much.'],
  'splits preview': [
    'Saves nothing. `--mode` is `equal`, `percent` (two decimals, 100.00 in all), `shares` (whole numbers) or `exact` (amounts that add up to the bill). `--mine` is your value; in `percent` and `exact` leave it out to take the rest.',
    '`--values` has one value per friend in `--people` order, then one per guest in `--guests` order. Parts add up to the cent; odd cents go to the largest remainders, you first.',
  ],
  'transactions split': [
    'Shares a paid expense with friends and guests (names for people you do not track; their part makes no debt). `myShare` is what it costs you: **budgets, statistics, reports and exports count that, not the amount.** The flags are those of `splits preview`.',
    'Changing the amount scales every part, so they still add up.',
  ],
  'transactions unsplit': ['It costs you the whole amount again; the friends\' balances go down by their parts.'],
  'recurring split': ['Splits every payment from now on, kept as proportions. `exact` needs an item with an amount. Payments already made keep theirs. Income cannot be split.'],
  'recurring unsplit': ['Payments already made keep their split.'],
  'export debts': ['Saves who owes what to a file; the app sends it to nobody. `--what` is `summary`, `statement` (one friend, with `--person-id`) or `ledger` (CSV only). `--format` is `text`, `csv` or `pdf`. It holds names and amounts and is not encrypted. You allow it in the app first.'],
  'export preview': ['Writes and asks nothing: counts what an export with these choices would hold. Same choices as `export json`.'],
  'export json': ['The whole data as one file, with the change history unless limited. Documents are listed, never their files. Each transaction has `amount` (paid) and `myShare` (cost to you). Limits: dates, categories, `--kind`, `--min-amount` and `--max-amount` (what it cost you), `--text`, `--split`. `--sections` picks parts, comma-separated: transactions, categories, budgets, recurring, documents, people, due, ledger, history. With a limit, the debt history is left out unless you give `--include-debts` or name `ledger`. Not encrypted.'],
  'export csv': ['One row per transaction, with a byte order mark for Excel, the attached document names and a `my_share` column. Takes the limits of `export json`, no parts. Text that starts like a formula is written as text, so opening the file cannot run anything.'],
  'export xlsx': ['One sheet each for transactions, categories, budgets, recurring items, a monthly summary, documents (listed), friends, due payments, debt history and bill parts. Takes the limits and `--sections` of `export json`. Dates are real dates, amounts are numbers.'],
  'export pdf': ['A printable report of whole months, in English. Leave out `--show-descriptions` to share without private details. `--include-debts`, `--include-due-payments` and `--include-debt-history` add friends\' names and amounts.'],
  'backup create': [
    'One encrypted file with everything except the files of the documents (a backup made in the app can include them). `--passphrase -` reads it from standard input so it is not in the process list. Without the passphrase the backup cannot be opened.',
    'Restoring is done in the app, never with a token.',
  ],
  'backup schedule': ['Setting it up is done in the app.'],
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
    if (c.positional) notes.push(`The ${c.positional} can also be a plain word: \`meowny ${c.command} 42\`.`)
    if (c.confirm) notes.push('The app asks you to allow this each time. See [Files need your yes](#files).')
    if (c.destructive) notes.push('Cannot be undone, so it needs `--yes`.')
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
      { t: 'p', text: `${commands.length} commands, each \`meowny <noun> <verb>\` or one word, then flags. \`--help\` works at every level, with no app running.` },
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
      { t: 'p', text: 'Every command ends with one of these exit codes, so a script can tell what happened.' },
      { t: 'table', head: ['Exit code', 'Means'], rows: Object.entries(data.exitCodes).map(([code, text]) => [`**${code}**`, text]) },
      { t: 'p', text: 'With `--json`, an error is one line on standard error: `{"code": "...", "message": "..."}`. The `code` is stable; the message is for people.' },
      { t: 'table', head: ['Code', 'Exit', 'What it means and what to do'], rows: errorCodes.map(([code, exit]) => [`\`${code}\``, String(exit), MEANINGS[code]]) },
      { t: 'p', text: 'Recorded from the program:' },
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
      { t: 'p', text: 'The token is looked for in this order: `--token-file`, `MEOWNY_TOKEN`, `MEOWNY_TOKEN_FILE`, then the file `meowny login` saved. A token file others can read is refused.' },
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
      { t: 'p', text: 'The command line is a thin client over a small HTTP API the app serves on this computer only (`127.0.0.1`, a port written to `api.json` in its data folder). It is off until you turn it on.' },
      {
        t: 'ul',
        items: [
          'Every call is `POST /v1/<route>` with a JSON body (none means `{}`) and `Authorization: Bearer <token>`.',
          'The answer is `{"ok": true, "data": ...}` or `{"ok": false, "error": {"code": "...", "message": "..."}}`, with a matching HTTP status.',
          '`Host` must be `127.0.0.1:<port>` or `localhost:<port>`, and a request with an `Origin` header is refused, so a web page cannot use it.',
          'Money is text (`"12.50"`), dates `YYYY-MM-DD`, months `YYYY-MM`. Unknown arguments are a 400 that names them.',
          'Routes marked "and your yes" wait up to two minutes for you in the app, and answer `denied` if you refuse.',
          'A body is at most 1 MiB, except `documents.add`: the file goes in `data` as base64url text, up to 35 MiB, read only after the token and its scope are checked.',
        ],
      },
      { t: 'code', label: 'Ask the app for its status', text: `curl -s -X POST "http://127.0.0.1:$PORT/v1/status" \\\n  -H "Authorization: Bearer $MEOWNY_TOKEN"\n\n{"data":${statusJson},"ok":true}` },
      { t: 'p', text: '`meowny schema --app` prints every route with its arguments as JSON.' },
      { t: 'table', head: ['Route', 'Token', 'Arguments', 'What it does'], rows: httpRows, cls: 'routes' },
    ],
  }

  // ---- the rest of the prose ----
  const guide = [
    {
      id: 'overview',
      title: 'Overview',
      blocks: [
        { t: 'p', text: '`meowny` drives the meowNY desktop app from a terminal, a script or an AI agent. It never opens your data file: it asks the running app over a connection only this computer can reach, and the app applies the same rules as its own screens.' },
        { t: 'ul', items: ['**Read:** transactions, budgets, recurring items, statistics.', '**Change:** record a transaction, set a budget, pay a recurring bill.', '**Documents:** add, attach, list, and get a copy back with your yes.', '**Take data out:** JSON, CSV, Excel, PDF or an encrypted backup, each only when you allow it.'] },
        { t: 'note', kind: 'info', text: 'The app must be running and unlocked, with command line access turned on in Settings (it is off by default). The phone app has no command line.' },
      ],
    },
    {
      id: 'quick-start',
      title: 'Quick start',
      blocks: [
        { t: 'p', text: '**1. Turn it on.** In the app: Settings, *command line access*, tick *allow the command line to connect*.' },
        { t: 'p', text: '**2. Make a token.** Choose a name, read only or read and write, and a lifetime. Copy it now: it is shown once.' },
        { t: 'p', text: '**3. Install.** For now, build from the source folder (needs Rust):' },
        { t: 'code', text: 'cargo install --path cli' },
        { t: 'p', text: '**4. Give it the token,** in the environment, or save it once:' },
        { t: 'code', text: 'export MEOWNY_TOKEN=mny_...\n\n# or, once, from standard input (never on a command line):\necho "$TOKEN" | meowny login' },
        { t: 'p', text: '**5. Try it.**' },
        { t: 'code', text: 'meowny status\nmeowny transactions list --limit 5\nmeowny stats budget --month 2026-10' },
        { t: 'note', kind: 'info', text: 'Start with a read-only token. A token that can write can add, change and delete your data until it expires or you revoke it.' },
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
            'A token is a secret that starts with `mny_`, made in the app. Treat it like a password.',
            '**Read only** unless you choose read and write (which also reads). It lasts 1 to 365 days (30 by default) and can be revoked at any time, at once.',
            'The app keeps only a fingerprint, so tokens cannot be read back from your data file.',
            'There is no `--token` flag, because other users can read a command line. Use `MEOWNY_TOKEN`, `--token-file` or `meowny login`.',
          ],
        },
        { t: 'h3', id: 'files', text: 'Files need your yes' },
        { t: 'p', text: 'An export, a backup or a copy of a document writes a file, so a token alone is not enough. The app shows a dialog with the token, what will be written, the exact file and what is in it. Nothing is written until you click Allow.' },
        {
          t: 'ul',
          items: [
            'The file needs the right ending (`.json`, `.csv`, `.xlsx`, `.pdf`, `.meowbak`, or a document\'s own), a folder that exists, and **must not exist yet**. A file is never replaced.',
            'A relative `--out` is made absolute from where you run the program; the dialog shows the full path.',
            'One question at a time. After a No the same token is refused for 30 seconds. No answer in two minutes counts as No.',
            'On Linux and macOS the files are readable only by you. They are not encrypted, except a backup (your passphrase).',
            'Restoring a backup, changing the currency, lock settings, tokens, scheduling a backup and deleting everything are app-only.',
          ],
        },
        { t: 'h3', id: 'locked', text: 'A locked app, a closed app' },
        {
          t: 'ul',
          items: [
            'Locked: every command answers `locked` (exit code 3) and prints no data. The command line cannot unlock the app or keep it awake.',
            'Not running, or command line access off: `app_unavailable` (exit code 1).',
          ],
        },
        { t: 'h3', id: 'output', text: 'What is printed' },
        {
          t: 'ul',
          items: [
            'Without `--json`: a plain table or `key: value` lines.',
            'With `--json`: one line of JSON, exactly as the app sent it; an error is a JSON line on standard error.',
            'Data goes to standard output and messages to standard error, so `meowny ... | jq` works. A longer list says on standard error where the next page starts.',
          ],
        },
        { t: 'h3', id: 'conventions', text: 'Conventions' },
        {
          t: 'ul',
          items: [
            '**Money** is text with two decimals (`12.50`), never a float.',
            '**Ids** are text. **Dates** are `YYYY-MM-DD`, **months** `YYYY-MM`, in the time zone set in the app.',
            '**Words** such as `expense` or `INCOME` ignore case.',
            '**Deleting** (`delete`, `remove`, `end`) needs `--yes`.',
            '**Clearing** a field is `--clear-<flag>`; a flag left out leaves the field as it is.',
            'A text flag given `-` reads its value from standard input. The program never prompts.',
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
          'Give it a **read-only token** in `MEOWNY_TOKEN`; read and write only if it must change things. On exit code 3 with `unauthorized`, ask for a new token.',
          'Run `meowny schema` once: offline JSON of every command, flag, type, token scope, `--yes` need, exit code and error code.',
          'Always pass `--json`. Data is on standard output; errors, as `{code, message}`, on standard error. Branch on the **exit code** (0 done, 1 error, 2 bad usage, 3 token or locked, 4 not found).',
          'Create transactions with your own `--client-uuid`: a retry cannot record one twice.',
          'Exports, backups and document copies wait for the person to allow them in the app. Tell the user to look, and do not retry in a loop. Exit code 3 with `denied` means no.',
          'Never print the token or put it in a command line, a shared file or a chat.',
        ],
      },
      {
        t: 'code',
        label: 'A paragraph to paste into the agent\'s instructions',
        text: 'You can read and update the user\'s budget with the `meowny` command (the meowNY desktop app must be running and unlocked).\nThe token is in $MEOWNY_TOKEN. Run `meowny schema` once to learn the commands and flags.\nAlways add --json. Data is on stdout; errors are JSON {code, message} on stderr; the exit code is 0 ok,\n1 error, 2 bad usage, 3 token refused or app locked, 4 not found. Amounts are strings like "12.50".\nCreate transactions with your own --client-uuid so a retry cannot duplicate them. Deleting needs --yes: only\ndo it when the user asked. Exports, backups and copies of documents need the user to click Allow in the app: ask them to, and\ndo not retry after a denial. Never reveal the token.',
      },
      { t: 'p', text: 'This page is also plain Markdown: [CLI.md](CLI.md).' },
    ],
  }

  const security = {
    id: 'security',
    title: 'Security',
    blocks: [
      { t: 'ul', items: [
        '**Off by default.** Nothing listens until you turn command line access on.',
        '**This computer only.** `127.0.0.1`, never the network.',
        '**A token for every request,** checked against fingerprints in your encrypted data. A locked app gives nothing.',
        '**Web pages cannot use it:** a request with an `Origin` header or the wrong `Host` is refused.',
        '**Least power.** Tokens are read only unless you choose otherwise, expire, and can be revoked.',
        '**Your yes for files,** each time, with the exact file shown, never over an existing file.',
        '**No restore, wipe or currency change** through it, whatever the token.',
        '**It does not keep the app awake,** so the idle lock still works.',
        '**Nothing leaves the computer.** No telemetry.',
      ] },
      { t: 'note', kind: 'warn', text: 'A leaked token is a key to your data. Read only lets someone who can reach this computer read everything until it expires; read and write lets them change it too. Revoke it in Settings, command line access, as soon as you suspect it. The saved token file is protected only by file permissions.' },
      { t: 'p', text: `Report a problem on [the project page](${REPO}).` },
    ],
  }

  const sections = [...guide, commandSection, referenceSection, errorSection, agents, httpSection, security]

  // ---- the side navigation ----
  const nav = [
    { title: 'Basics', links: [{ id: 'overview', text: 'Overview' }, { id: 'quick-start', text: 'Quick start' }, { id: 'concepts', text: 'How it works' }, { id: 'tokens', text: 'Tokens' }, { id: 'files', text: 'Files need your yes' }, { id: 'conventions', text: 'Conventions' }] },
    { title: 'Commands', href: 'commands', cls: 'commands', links: [] },
    ...[...groups].map(([key, g]) => ({ title: g.title, href: `group-${key}`, cls: 'commands', links: g.commands.map((c) => ({ id: `cmd-${c.command.replace(/ /g, '-')}`, text: key === 'general' ? c.command : c.command.split(' ')[1] })) })),
    { title: 'Reference', links: [{ id: 'reference', text: 'Flags and settings' }, { id: 'errors', text: 'Exit codes and errors' }, { id: 'agents', text: 'For AI agents' }, { id: 'http-api', text: 'The HTTP API' }, { id: 'security', text: 'Security' }] },
  ]

  return {
    file: 'cli.html',
    title: 'meowny command line',
    description: 'Use your meowNY budget from a terminal, a script or an AI agent. Commands, flags, examples, exit codes and the HTTP API behind it.',
    lead: 'Use your meowNY budget from a terminal, a script or an AI agent. It talks to the desktop app on the same computer; your data never leaves it.',
    footer: `meowNY is free software (AGPL-3.0). Source: [${REPO.replace('https://', '')}](${REPO}). Generated from \`meowny\` ${cli.version} (API ${cli.api}); the examples are real output.`,
    nav,
    sections,
  }
}
