# meowny command line

Use your meowNY budget from a terminal, a script or an AI agent. The command line talks to the desktop app on the same computer; your data never leaves it.

## Overview

`meowny` is a small program that drives the meowNY desktop app, for scripts, for AI agents and for anyone who prefers a terminal. It never opens your data file. It asks the running app, over a connection only this computer can reach, and the app does the work with the same rules as its own screens: the same amounts, the same budgets, the same checks.

- **Look:** list transactions, budgets and recurring items; see budget against actual, the year, the top categories.
- **Change:** record a transaction, set a budget, pay a recurring bill.
- **Documents:** add a receipt or a contract from a file, attach it to a transaction, list what is there and, with your yes, get a copy back.
- **Take data out:** export to JSON, CSV, Excel or PDF, or make an encrypted backup, each only when you allow it in the app.

> **Note:** The app has to be running and unlocked, and command line access has to be turned on in its Settings. It is off by default. The phone app has no command line.

## Quick start

**1. Turn it on.** In the app, open Settings, find *command line access* and tick *allow the command line to connect*.

**2. Make a token.** In the same block choose a name, read only or read and write, and how long it lasts, then *make token*. Copy it now: it is shown once, and the app keeps only a fingerprint of it.

**3. Install the program.** For now, build it from the source folder (you need Rust):

```
cargo install --path cli
```

**4. Give it the token,** in the environment, or save it once:

```
export MEOWNY_TOKEN=mny_...

# or, once, from standard input (never put it on a command line):
echo "$TOKEN" | meowny login
```

**5. Try it.**

```
meowny status
meowny transactions list --limit 5
meowny stats budget --month 2026-10
```

> **Note:** Start with a read-only token. A token that can write can add, change and delete your data until it expires or you revoke it in Settings.

## How it works

### Tokens

A token is a long secret that starts with `mny_`. You make it in the app. It is **read only** unless you choose read and write (which also reads), it lasts 1 to 365 days (30 by default), and you can revoke it at any time; it stops working at once. The app stores only a fingerprint of it, so nobody can read tokens back from your data file.

There is deliberately no `--token` flag: a command line can be read by other users of the computer. Use `MEOWNY_TOKEN`, `--token-file`, or `meowny login`.

### Files need your yes

An export, a backup or a copy of a document writes a file on your computer, so a token alone is not enough. Each request shows a dialog in the app: which token is asking, what it will write, the exact file, and what is in it (how many transactions, or which document). Nothing is written until you click Allow.

- The file must have the right ending (`.json`, `.csv`, `.xlsx`, `.pdf`, `.meowbak`, or for a document the extension of its kind), be in a folder that exists, and **not exist yet**. A file is never replaced.
- A relative `--out` is made absolute from where you run the program, and the dialog shows the full path.
- One question at a time. After a No, the same token is refused for 30 seconds, so a script cannot wear you down. No answer in two minutes counts as No.
- On Linux and macOS the files are readable only by you. They are not encrypted, except a backup, which is locked with the passphrase you give.
- Restoring a backup, changing the currency, the lock settings, tokens, setting up a scheduled backup and deleting everything cannot be done through the command line at all. They are done in the app.

### A locked app, a closed app

If the app is locked (it locks itself after a while, or when you minimise it, as you set it up), every command answers `locked` (exit code 3) and prints no data. The command line cannot unlock it, and using it does not keep the app awake. If the app is not running, or command line access is off, you get `app_unavailable` (exit code 1).

### What is printed

Without `--json` you get a plain table or `key: value` lines, and messages (such as "done.") on standard error. With `--json` the data is one line of JSON on standard output, exactly as the app sent it, and an error is a JSON line on standard error. Data is always on standard output and messages on standard error, so `meowny ... | jq` works. A page of a longer list says on standard error where the next page starts.

### Conventions

- **Money** is text with two decimals, like `12.50`. It is never a floating-point number anywhere, so nothing is rounded on the way.
- **Ids** are text, as the app lists them. **Dates** are `YYYY-MM-DD`, **months** `YYYY-MM`, in the time zone set in the app.
- **Words** such as `expense` or `INCOME` can be written in any case.
- **Deleting** (`delete`, `remove`, `end`) needs `--yes`.
- **Clearing** a field uses `--clear-<flag>`; leaving a flag out leaves the field as it is.
- A text flag given `-` reads its value from standard input.
- The program never prompts and never waits for input you did not ask it to read (`login` reads the token from standard input, and a text flag given `-` reads its value there).

## Commands

Every command is `meowny <noun> <verb>`, or a single word, with its flags after it. `--help` works at every level, with no app running. There are 70 commands; each is listed here with what it prints.

| Command | What it does | Needs |
| --- | --- | --- |
| [`status`](#cmd-status) | The app version, the API version and the scope of the token used. | read token |
| [`schema`](#cmd-schema) | Every route with its scope and arguments, so a client can discover them. | read token |
| [`today`](#cmd-today) | Today's date and the current month in the time zone set in the app. | read token |
| [`settings get`](#cmd-settings-get) | The currency, number format, time zone and language set in the app. | read token |
| [`categories list`](#cmd-categories-list) | The categories that are in use, oldest first, with their ids. | read token |
| [`transactions list`](#cmd-transactions-list) | Transactions, newest first, with the total that match. Up to 100 a page, 50 unless limit says otherwise. | read token |
| [`budgets overview`](#cmd-budgets-overview) | For a month, per side (income, expense): the budget, how much of it is given to categories, what is left, and the actual so far. | read token |
| [`budgets history`](#cmd-budgets-history) | One row per month, newest first, from the first month that had a budget: the budget and the actual for each side. | read token |
| [`recurring list`](#cmd-recurring-list) | Every recurring item, with its schedule, its next due date and whether it is active. | read token |
| [`recurring dues`](#cmd-recurring-dues) | Active recurring items that are overdue (due before today) and those due in the next days. Today is the date in the app's time zone. | read token |
| [`stats budget`](#cmd-stats-budget) | Budget against actual for a month: each category with its budget, what was spent or earned, what is left and the percent used, and the totals. | read token |
| [`stats trend`](#cmd-stats-trend) | One category month by month, with its budget and actual for each month. At most 120 months. | read token |
| [`stats year`](#cmd-stats-year) | Income, expense and savings for each month of a year. | read token |
| [`stats top`](#cmd-stats-top) | The categories with the most spending, or income, in a month, biggest first, with their share of the month. | read token |
| [`stats commitments`](#cmd-stats-commitments) | What the active recurring items cost, or bring in, in an average month. Items with no fixed amount are listed but not counted. | read token |
| [`stats planned`](#cmd-stats-planned) | Planned against paid amounts of the recurring items that were paid in a month. | read token |
| [`settings set`](#cmd-settings-set) | Changes the number and date format, the time zone or the language. Give at least one. The currency is changed in the app. | write token |
| [`categories create`](#cmd-categories-create) | Makes a category. Its name is 1 to 50 characters and unique among the categories of that type. | write token |
| [`categories rename`](#cmd-categories-rename) | Renames a category. Its type cannot change. | write token |
| [`categories delete`](#cmd-categories-delete) | Deletes a category: it is no longer offered or budgeted, and its transactions keep it. | write token |
| [`transactions create`](#cmd-transactions-create) | Records a transaction. With a `--client-uuid`, sending the same request again gives back the first transaction instead of making another. | write token |
| [`transactions update`](#cmd-transactions-update) | Changes a transaction. Fields left out stay as they are; a null description clears it. | write token |
| [`transactions delete`](#cmd-transactions-delete) | Deletes a transaction. The change history keeps what it said. | write token |
| [`budgets set`](#cmd-budgets-set) | Sets a monthly budget: the global budget of a side, or one category's. A month that has already started changes only that month; this month and later ones carry the new amount forward. A category budget must match the category's type. | write token |
| [`budgets remove`](#cmd-budgets-remove) | Removes a budget, with the same month rules as setting one. The months before keep it, and the history stays. | write token |
| [`recurring create`](#cmd-recurring-create) | Adds a recurring item: a bill or an income that comes back every so many days, weeks, months or years. | write token |
| [`recurring update`](#cmd-recurring-update) | Changes a recurring item. Fields left out stay as they are; null clears the end date or the description, and a null amount makes it a bill with no fixed amount. | write token |
| [`recurring delete`](#cmd-recurring-delete) | Deletes a recurring item. The transactions it already made stay. | write token |
| [`recurring pay`](#cmd-recurring-pay) | Turns the current occurrence into a transaction and moves the item to the next one. A bill with no fixed amount needs the amount. | write token |
| [`recurring skip`](#cmd-recurring-skip) | Passes over the current occurrence without making a transaction. | write token |
| [`recurring pause`](#cmd-recurring-pause) | Pauses a recurring item: it stops coming due until it is resumed. | write token |
| [`recurring resume`](#cmd-recurring-resume) | Resumes a paused recurring item, unless its next date is past its end date. | write token |
| [`recurring end`](#cmd-recurring-end) | Ends a recurring item for good: it becomes inactive and its end date is today. What it already paid stays. | write token |
| [`export preview`](#cmd-export-preview) | What an export of this selection would contain: how many transactions, their dates, the income and expense totals, and the other records (the friends, the history of debts, the overdue payments, the documents). | read token |
| [`backup schedule`](#cmd-backup-schedule) | The scheduled backup, if one is set up: the folder, how often and how the last one went, or null. Its passphrase is never shown. | read token |
| [`export json`](#cmd-export-json) | Writes the data as a JSON file, after the person allows it in the app: the transactions, categories, budgets, recurring items, documents, and the debts (friends, the overdue recurring payments, the history of debts), or the parts and the limits chosen. An existing file is never replaced. | read token, and your yes |
| [`export csv`](#cmd-export-csv) | Writes the transactions as a CSV file, after the person allows it in the app, limited by the dates, the categories, the kind, what they cost you, a text and whether they are shared. An existing file is never replaced. | read token, and your yes |
| [`export xlsx`](#cmd-export-xlsx) | Writes a workbook (.xlsx) with sheets for transactions, categories, budgets, recurring items, documents, the debts (friends, due payments, history of debts) and a monthly summary, after the person allows it in the app. Dates are ISO and amounts plain numbers. An existing file is never replaced. | read token, and your yes |
| [`export pdf`](#cmd-export-pdf) | Writes a PDF report for whole months, in English, after the person allows it in the app: totals, charts and budget against actual, and the transactions if asked. An existing file is never replaced. | read token, and your yes |
| [`backup create`](#cmd-backup-create) | Writes an encrypted backup of all the data, after the person allows it in the app. It opens only with the passphrase given here, and only the app can restore it. An existing file is never replaced. | read token, and your yes |
| [`documents list`](#cmd-documents-list) | Documents, newest paper first, with the total that match: names, dates, types, sizes and hashes, never the files. Up to 100 a page, 50 unless limit says otherwise. | read token |
| [`documents show`](#cmd-documents-show) | One document: its name, date, type, size and hash, whether this computer holds its file, and in `links` what it is attached to (a debt entry also gives `personId`, the friend). Never the file. | read token |
| [`documents add`](#cmd-documents-add) | Stores a file as a document, encrypted, and attaches it where asked. The answer is the document and `alreadyThere`: the same file added again is not stored twice, and the attachments are added to the document that is there. At most 25 MiB. | write token |
| [`documents update`](#cmd-documents-update) | Renames a document, changes its date, or switches whether the phone keeps its file. Give at least one. The contents cannot be changed. | write token |
| [`documents delete`](#cmd-documents-delete) | Deletes a document, its file and its attachments. The transactions it was attached to stay. | write token |
| [`documents link`](#cmd-documents-link) | Attaches a document to a transaction, a recurring item or a ledger entry. Giving one that is attached already changes nothing. | write token |
| [`documents unlink`](#cmd-documents-unlink) | Detaches a document from a transaction, a recurring item or a ledger entry. The document and the transaction stay. | write token |
| [`documents get`](#cmd-documents-get) | Writes a copy of a document's file, after the person allows it in the app. It is not encrypted. An existing file is never replaced. | write token |
| [`people list`](#cmd-people-list) | The friends with what each owes you (negative: you owe them), the largest first. Archived friends only if asked. | read token |
| [`people show`](#cmd-people-show) | One friend and their ledger: their parts of shared bills and the entries by date, with the balance after each line. | read token |
| [`debts summary`](#cmd-debts-summary) | What friends owe you and what you owe them, in all and for each friend whose balance is not zero. Neither is income or expense. | read token |
| [`debts due`](#cmd-debts-due) | The recurring payments that are overdue and not paid or skipped, one for each occurrence that was missed (at most 24 for an item), the oldest first, with the day each fell due, what it is for (null for a bill with no fixed amount), the days late and what they come to. A payment is overdue from the day after its day; income and paused or ended items are not listed. | read token |
| [`debts history`](#cmd-debts-history) | The history of debts, newest first, a page at a time: the lines of every friend (lent, borrowed, paid back, their parts of shared bills, with the balance after each) and the recurring payments that were paid late or are overdue. A payment made on its day or before it is not in it. | read token |
| [`debts reminders`](#cmd-debts-reminders) | Which friends a reminder is due for today, and the next days with one. Every friend whose balance is not zero is reminded this many days after the day their debt last changed (the latest line of their statement), and again after as many; 0 is none. The app rings them (a device sets its own period and time of day); this only reads what they would be about. | read token |
| [`splits preview`](#cmd-splits-preview) | What a split would give each person for a bill of this amount, without saving anything: your part, theirs, whether the numbers add up and how much is left. | read token |
| [`people create`](#cmd-people-create) | Adds a friend. A friend is a name, 1 to 50 characters, unique among the friends who are not archived. Nothing else about a person is kept. | write token |
| [`people update`](#cmd-people-update) | Renames a friend. Fields left out stay as they are. The reminder fields are kept for data written before reminders became a setting of each device (every so many days after a debt last changed), and nothing rings from them. | write token |
| [`people archive`](#cmd-people-archive) | Hides a friend from the lists and pickers. Their balance and history stay, and the name can be used again. | write token |
| [`people unarchive`](#cmd-people-unarchive) | Brings back an archived friend, if no other friend has taken the name. | write token |
| [`people settle`](#cmd-people-settle) | Marks a payment between you and a friend: they paid you back if they owe you, you paid them back if you owe them. With no amount it is the whole balance. An amount larger than the debt is accepted: the balance turns around and the answer says how much was over. | write token |
| [`ledger add`](#cmd-ledger-add) | Records money lent, borrowed or paid back with a friend. It is never income or expense. For a borrowing, give `--expense-name` and `--expense-category-id` to add your share to your expenses in the same step, with the entry linked to it. | write token |
| [`ledger update`](#cmd-ledger-update) | Changes an entry of the ledger. Fields left out stay as they are; a null note or `--transaction-id` clears it. The expense it came with is not changed. | write token |
| [`ledger delete`](#cmd-ledger-delete) | Deletes an entry of the ledger. The balance of the friend changes by that much. The change history keeps what it said. | write token |
| [`transactions split`](#cmd-transactions-split) | Shares a transaction with friends and guests. It keeps the amount that was paid; your part, the amount minus theirs, is what budgets and statistics count. The numbers must add up. Only an expense can be shared. | write token |
| [`transactions unsplit`](#cmd-transactions-unsplit) | Takes the split off a transaction: it costs you its whole amount again, and the friends' balances go down by their parts. | write token |
| [`recurring split`](#cmd-recurring-split) | Splits every payment of a recurring item the same way, from now on. Kept as proportions, so a payment that differs from the plan is shared in the same proportions. EXACT needs an item with an amount that the values add up to. Payments already made are not changed. | write token |
| [`recurring unsplit`](#cmd-recurring-unsplit) | Takes the split off a recurring item: its payments are not split from now on. Payments already made keep theirs. | write token |
| [`export debts`](#cmd-export-debts) | Writes who owes what to a text, CSV or PDF file, in English, after the person allows it in the app: the SUMMARY of all friends, the STATEMENT of one friend (`--person-id`), or the LEDGER of every entry as CSV. An existing file is never replaced. | read token, and your yes |
| [`login`](#cmd-login) | Saves a token, read from standard input, for the commands that follow. | this computer |
| [`logout`](#cmd-logout) | Removes the saved token. | this computer |

### General

#### `status`

*read token*

The app version, the API version and the scope of the token used.

```text
meowny status
```

```console
$ meowny status

api: 1
app: meowny
schemaVersion: 13
scope: read
version: 0.1.0
```

With `--json` (one line; shown indented):

```json
{
  "api": 1,
  "app": "meowny",
  "schemaVersion": 13,
  "scope": "read",
  "version": "0.1.0"
}
```

#### `today`

*read token*

Today's date and the current month in the time zone set in the app.

```text
meowny today
```

```console
$ meowny today

date: 2026-10-18
month: 2026-10
```

With `--json` (one line; shown indented):

```json
{
  "date": "2026-10-18",
  "month": "2026-10"
}
```

#### `login`

*this computer*

Saves a token, read from standard input, for the commands that follow.

```text
meowny login
```

Reads one line from standard input, so the token never appears in a command line (which other users can read). On Linux and macOS the file is written readable only by you, and a token file that other users can read is refused; on Linux it is `~/.config/meowny/token`.

```console
$ echo "$MEOWNY_TOKEN" | meowny login

token saved to /home/you/.config/meowny/token
```

#### `logout`

*this computer*

Removes the saved token.

```text
meowny logout
```

Removes the saved token. A token in `MEOWNY_TOKEN` is not touched.

```console
$ meowny logout

token removed from /home/you/.config/meowny/token
```

#### `schema`

*read token*

Every route with its scope and arguments, so a client can discover them.

```text
meowny schema
```

Answers from what the program was built with, so it works with no app and no token. With `--app` it asks the running app for its own list of routes instead (needs a token).

```console
$ meowny schema

{"cli":{"api":1,"name":"meowny","version":"0.1.0"},"commands":[{"command":"status","confirm":false,"destructive":false,"flags":[],"positional":null,"route":"status","scope":"read","summary":"The app version, the API version and the scope of the token used.","usage":"meowny status"},{"command":"schema","confirm":false,"destructive":false,"flags":[],"positional":null,"route":"schema","scope":"read","summary":"Every route with its scope and arguments, so a client can discover them.","usage":"meowny schema"},{"command":"today","confirm":false,"destructive":false,"flags":[],"positional":null,"route":"today","scope":"read","summary":"Today's date and the current month in the time zone set in the a…
```

### Settings

#### `settings get`

*read token*

The currency, number format, time zone and language set in the app.

```text
meowny settings get
```

```console
$ meowny settings get

currency: EUR
language: en
locale: en-IE
timezone: Europe/Dublin
```

With `--json` (one line; shown indented):

```json
{
  "currency": "EUR",
  "language": "en",
  "locale": "en-IE",
  "timezone": "Europe/Dublin"
}
```

#### `settings set`

*write token*

Changes the number and date format, the time zone or the language. Give at least one. The currency is changed in the app.

```text
meowny settings set [--locale <text>] [--timezone <text>] [--language <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--locale` | text | How numbers and dates are written, like en-IE. |
| `--timezone` | text | A time zone name, like Europe/Dublin. |
| `--language` | text | The language of the app, like en or ro. |

The currency is not here: changing it can convert every amount, so it is done in the app.

```console
$ meowny settings set --language ro
```

With `--json` (one line; shown indented):

```json
{
  "currency": "EUR",
  "language": "ro",
  "locale": "en-IE",
  "timezone": "Europe/Dublin"
}
```

### Categories

#### `categories list`

*read token*

The categories that are in use, oldest first, with their ids.

```text
meowny categories list [--type <INCOME|EXPENSE>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--type` | one of INCOME, EXPENSE | INCOME or EXPENSE. |

```console
$ meowny categories list --type expense

id  name                 type
 4  Rent                 EXPENSE
 5  Groceries            EXPENSE
 6  Transport            EXPENSE
 7  Eating out           EXPENSE
 8  Entertainment        EXPENSE
 9  Subscriptions        EXPENSE
10  Utilities            EXPENSE
11  Health               EXPENSE
12  Travel               EXPENSE
13  Gifts                EXPENSE
14  Școală și hobby-uri  EXPENSE
```

With `--json` (one line; shown indented):

```json
[
  {
    "id": "4",
    "name": "Rent",
    "type": "EXPENSE"
  },
  {
    "id": "5",
    "name": "Groceries",
    "type": "EXPENSE"
  },
  {
    "id": "6",
    "name": "Transport",
    "type": "EXPENSE"
  },
  {
    "id": "7",
    "name": "Eating out",
    "type": "EXPENSE"
  },
  {
    "id": "8",
    "name": "Entertainment",
    "type": "EXPENSE"
  },
  {
    "id": "9",
    "name": "Subscriptions",
    "type": "EXPENSE"
  },
  {
    "id": "10",
    "name": "Utilities",
    "type": "EXPENSE"
  },
  {
    "id": "11",
    "name": "Health",
    "type": "EXPENSE"
  },
  {
    "id": "12",
    "name": "Travel",
    "type": "EXPENSE"
  },
  {
    "id": "13",
    "name": "Gifts",
    "type": "EXPENSE"
  },
  {
    "id": "14",
    "name": "Școală și hobby-uri",
    "type": "EXPENSE"
  }
]
```

#### `categories create`

*write token*

Makes a category. Its name is 1 to 50 characters and unique among the categories of that type.

```text
meowny categories create --name <text> --type <INCOME|EXPENSE>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--name` (required) | text | The name. |
| `--type` (required) | one of INCOME, EXPENSE | INCOME or EXPENSE. |

```console
$ meowny categories create --name Pets --type expense
```

With `--json` (one line; shown indented):

```json
{
  "id": "16",
  "name": "Pets",
  "type": "EXPENSE"
}
```

#### `categories rename`

*write token*

Renames a category. Its type cannot change.

```text
meowny categories rename <id> --name <text>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The category. |
| `--name` (required) | text | The new name. |

The id can be given as a plain word, as in `meowny categories rename 42`, or with `--id`.

```console
$ meowny categories rename 16 --name Animals
```

With `--json` (one line; shown indented):

```json
{
  "id": "16",
  "name": "Animals",
  "type": "EXPENSE"
}
```

#### `categories delete`

*write token · needs --yes*

Deletes a category: it is no longer offered or budgeted, and its transactions keep it.

```text
meowny categories delete <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The category. |

The id can be given as a plain word, as in `meowny categories delete 42`, or with `--id`.

This cannot be undone, so it needs `--yes`.

```console
$ meowny categories delete 16 --yes

done.
```

### Transactions

#### `transactions list`

*read token*

Transactions, newest first, with the total that match. Up to 100 a page, 50 unless limit says otherwise.

```text
meowny transactions list [--from <date>] [--to <date>] [--category-id <id>] [--q <text>] [--limit <number>] [--offset <number>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--from` | date | Only on or after this date. |
| `--to` | date | Only on or before this date. |
| `--category-id` | id | Only this category. |
| `--q` | text | Only transactions whose name or description contains this text. |
| `--limit` | number | How many to return, 1 to 100. Default 50. |
| `--offset` | number | How many to skip, for the next page. Default 0. |

```console
$ meowny transactions list --limit 4

limit: 4
offset: 0
total: 423

transactions
 id  name             paymentDate  amount  categoryId  clientUuid  description  myShare
403  Coffee           2026-10-13     3.20           7  -           Flat white      3.20
402  Bus ticket       2026-10-13     2.80           6  -           -               2.80
401  Corner shop      2026-10-13     4.50           5  -           -               4.50
291  Board game cafe  2026-10-13    51.83           8  -           -              51.83

showing 1-4 of 423; next page: --offset 4
```

With `--json` (one line; shown indented):

```json
{
  "limit": 4,
  "offset": 0,
  "total": 423,
  "transactions": [
    {
      "amount": "3.20",
      "categoryId": "7",
      "clientUuid": null,
      "description": "Flat white",
      "id": "403",
      "myShare": "3.20",
      "name": "Coffee",
      "paymentDate": "2026-10-13",
      "split": []
    },
    {
      "amount": "2.80",
      "categoryId": "6",
      "clientUuid": null,
      "description": null,
      "id": "402",
      "myShare": "2.80",
      "name": "Bus ticket",
      "paymentDate": "2026-10-13",
      "split": []
    },
    {
      "amount": "4.50",
      "categoryId": "5",
      "clientUuid": null,
      "description": null,
      "id": "401",
      "myShare": "4.50",
      "name": "Corner shop",
      "paymentDate": "2026-10-13",
      "split": []
    },
    {
      "amount": "51.83",
      "categoryId": "8",
      "clientUuid": null,
      "description": null,
      "id": "291",
      "myShare": "51.83",
      "name": "Board game cafe",
      "paymentDate": "2026-10-13",
      "split": []
    }
  ]
}
```

#### `transactions create`

*write token*

Records a transaction. With a `--client-uuid`, sending the same request again gives back the first transaction instead of making another.

```text
meowny transactions create --name <text> --amount <amount> --payment-date <date> --category-id <id> [--description <text>] [--client-uuid <uuid>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--name` (required) | text | What it was, 1 to 50 characters. |
| `--amount` (required) | amount | More than zero, like 12.50. |
| `--payment-date` (required) | date | The day it was paid. |
| `--category-id` (required) | id | The category. Its type decides whether this is income or an expense. |
| `--description` | text | A note. |
| `--client-uuid` | uuid | A fresh UUID per transaction, so a retry cannot record it twice. |

A fresh `--client-uuid` is made if you do not give one, and it is in the answer. If a command fails or times out and you do not know whether the transaction was recorded, run it again with the same `--client-uuid`: the app returns the first transaction instead of making a second.

The category decides whether this is income or an expense.

```console
$ meowny transactions create --name Bread --amount 2.40 --payment-date 2026-10-14 --category-id 16 --client-uuid 9b2f5d44-6e1a-4c3b-8f2d-1a2b3c4d5e6f
```

With `--json` (one line; shown indented):

```json
{
  "amount": "2.40",
  "categoryId": "16",
  "clientUuid": "9b2f5d44-6e1a-4c3b-8f2d-1a2b3c4d5e6f",
  "description": null,
  "id": "424",
  "myShare": "2.40",
  "name": "Bread",
  "paymentDate": "2026-10-14",
  "split": []
}
```

#### `transactions update`

*write token*

Changes a transaction. Fields left out stay as they are; a null description clears it.

```text
meowny transactions update <id> [--name <text>] [--amount <amount>] [--payment-date <date>] [--category-id <id>] [--description <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The transaction. |
| `--name` | text | The new name. |
| `--amount` | amount | The new amount, more than zero. |
| `--payment-date` | date | The new date. |
| `--category-id` | id | The new category. |
| `--description` | text | The new note, or null to clear it. |
| `--clear-description` | no value | Clears description. |

Only the flags you give change. `--clear-description` removes the description.

The id can be given as a plain word, as in `meowny transactions update 42`, or with `--id`.

```console
$ meowny transactions update 424 --amount 2.95 --description Sourdough
```

With `--json` (one line; shown indented):

```json
{
  "amount": "2.95",
  "categoryId": "16",
  "clientUuid": "9b2f5d44-6e1a-4c3b-8f2d-1a2b3c4d5e6f",
  "description": "Sourdough",
  "id": "424",
  "myShare": "2.95",
  "name": "Bread",
  "paymentDate": "2026-10-14",
  "split": []
}
```

#### `transactions delete`

*write token · needs --yes*

Deletes a transaction. The change history keeps what it said.

```text
meowny transactions delete <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The transaction. |

The id can be given as a plain word, as in `meowny transactions delete 42`, or with `--id`.

This cannot be undone, so it needs `--yes`.

```console
$ meowny transactions delete 424 --yes

done.
```

#### `transactions split`

*write token*

Shares a transaction with friends and guests. It keeps the amount that was paid; your part, the amount minus theirs, is what budgets and statistics count. The numbers must add up. Only an expense can be shared.

```text
meowny transactions split <id> --mode <EQUAL|PERCENT|SHARES|EXACT> [--mine <text>] [--people <id,id,...>] [--guests <text>] [--values <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The transaction. |
| `--mode` (required) | one of EQUAL, PERCENT, SHARES, EXACT | How the numbers are typed: EQUAL (everyone the same), PERCENT (two decimals, 100.00 in all), SHARES (whole numbers) or EXACT (amounts that add up to the bill). |
| `--mine` | text | Your own value, typed the way the mode says. Leave out for an equal split, or to take the rest in PERCENT and EXACT. |
| `--people` | id,id,... | The friends the bill is shared with, as ids. |
| `--guests` | text | People who are not tracked, by name, separated by commas. Their part leaves your cost and makes no debt. |
| `--values` | text | The value of each friend, in the order of --people, then of each guest, separated by commas. Not for an equal split. |

Shares a transaction you paid with friends and guests (names typed for people you do not track; their part leaves your cost and makes no debt). It keeps the amount that was paid. `myShare` is what it costs you: **budgets, statistics, reports and exports count that, not the amount.** Only an expense can be shared. The flags are the ones of `splits preview`.

Changing the amount of a shared transaction scales every part, yours included, so they still add up to the cent.

The id can be given as a plain word, as in `meowny transactions split 42`, or with `--id`.

```console
$ meowny transactions split 425 --mode equal --people 5,6
```

With `--json` (one line; shown indented):

```json
{
  "amount": "90.00",
  "categoryId": "16",
  "clientUuid": "5c1d8e07-3b2a-4f6e-9a41-7d0c2b9e8f13",
  "description": null,
  "id": "425",
  "myShare": "30.00",
  "name": "Dinner",
  "paymentDate": "2026-10-12",
  "split": [
    {
      "amount": "30.00",
      "guest": null,
      "person": "Anna",
      "personId": "5"
    },
    {
      "amount": "30.00",
      "guest": null,
      "person": "Bob",
      "personId": "6"
    }
  ]
}
```

#### `transactions unsplit`

*write token*

Takes the split off a transaction: it costs you its whole amount again, and the friends' balances go down by their parts.

```text
meowny transactions unsplit <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The transaction. |

The transaction costs you its whole amount again, and the friends' balances go down by their parts.

The id can be given as a plain word, as in `meowny transactions unsplit 42`, or with `--id`.

```console
$ meowny transactions unsplit 425
```

With `--json` (one line; shown indented):

```json
{
  "amount": "90.00",
  "categoryId": "16",
  "clientUuid": "5c1d8e07-3b2a-4f6e-9a41-7d0c2b9e8f13",
  "description": null,
  "id": "425",
  "myShare": "90.00",
  "name": "Dinner",
  "paymentDate": "2026-10-12",
  "split": []
}
```

### Budgets

#### `budgets overview`

*read token*

For a month, per side (income, expense): the budget, how much of it is given to categories, what is left, and the actual so far.

```text
meowny budgets overview --month <month>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--month` (required) | month | The month, like 2026-10. |

```console
$ meowny budgets overview --month 2026-09

type     budgeted   actual  remaining  allocated  overAllocated  unallocated
INCOME    3600.00  3690.00     -90.00    3200.00  false               400.00
EXPENSE   2450.00  1961.64     488.36    2015.00  false               435.00
```

With `--json` (one line; shown indented):

```json
[
  {
    "actual": "3690.00",
    "allocated": "3200.00",
    "budgeted": "3600.00",
    "categories": [
      {
        "amountLimit": "3200.00",
        "categoryId": "1"
      }
    ],
    "overAllocated": false,
    "remaining": "-90.00",
    "type": "INCOME",
    "unallocated": "400.00"
  },
  {
    "actual": "1961.64",
    "allocated": "2015.00",
    "budgeted": "2450.00",
    "categories": [
      {
        "amountLimit": "950.00",
        "categoryId": "4"
      },
      {
        "amountLimit": "500.00",
        "categoryId": "5"
      },
      {
        "amountLimit": "70.00",
        "categoryId": "6"
      },
      {
        "amountLimit": "200.00",
        "categoryId": "7"
      },
      {
        "amountLimit": "45.00",
        "categoryId": "9"
      },
      {
        "amountLimit": "250.00",
        "categoryId": "10"
      }
    ],
    "overAllocated": false,
    "remaining": "488.36",
    "type": "EXPENSE",
    "unallocated": "435.00"
  }
]
```

#### `budgets history`

*read token*

One row per month, newest first, from the first month that had a budget: the budget and the actual for each side.

```text
meowny budgets history
```

```console
$ meowny budgets history

month    expenseActual  expenseBudget  incomeActual  incomeBudget
2026-10        1485.45        2450.00          0.00       3600.00
2026-09        1961.64        2450.00       3690.00       3600.00
2026-08        3992.69        2450.00       3240.00       3600.00
2026-07        1961.20        2450.00       3253.11       3600.00
2026-06        1955.38        2450.00       3520.00       3600.00
2026-05        2834.37        2450.00       3240.00       3600.00
2026-04        1904.86        2450.00       3214.22       3600.00
2026-03        2079.23        2300.00       3520.00       3600.00
2026-02        2162.65        2300.00       3200.00       3600.00
2026-01        1890.68        2300.00   …
```

With `--json` (one line; shown indented):

```json
[{"expenseActual":"1485.45","expenseBudget":"2450.00","incomeActual":"0.00","incomeBudget":"3600.00","month":"2026-10"},{"expenseActual":"1961.64","expenseBudget":"2450.00","incomeActual":"3690.00","incomeBudget":"3600.00","month":"2026-09"},{"expenseActual":"3992.69","expenseBudget":"2450.00","incomeActual":"3240.00","incomeBudget":"3600.00","month":"2026-08"},{"expenseActual":"1961.20","expenseBudget":"2450.00","incomeActual":"3253.11","incomeBudget":"3600.00","month":"2026-07"},{"expenseActual":"1955.38","expenseBudget":"2450.00","incomeActual":"3520.00","incomeBudget":"3600.00","month":"2026-06"},{"expenseActual":"2834.37","expenseBudget":"2450.00","incomeActual":"3240.00","incomeBudget"…
```

#### `budgets set`

*write token*

Sets a monthly budget: the global budget of a side, or one category's. A month that has already started changes only that month; this month and later ones carry the new amount forward. A category budget must match the category's type.

```text
meowny budgets set --type <INCOME|EXPENSE> --month <month> --amount-limit <amount> [--category-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--type` (required) | one of INCOME, EXPENSE | INCOME or EXPENSE. |
| `--month` (required) | month | The month, like 2026-10. |
| `--amount-limit` (required) | amount | The most to spend, or the aim to earn, each month. Zero is allowed. |
| `--category-id` | id | The category. Leave out for the global budget of that side. |

Without `--category-id` this is the global budget of that side (income or expense). A month that has already started changes only that month; this month and later ones carry the new amount forward, so past months keep what they showed.

```console
$ meowny budgets set --type expense --month 2026-12 --amount-limit 100.00 --category-id 16
```

With `--json` (one line; shown indented):

```json
{
  "amountLimit": "100.00",
  "categoryId": "16",
  "effectiveFrom": "2026-12-01",
  "id": "15",
  "scope": "CATEGORY",
  "type": "EXPENSE"
}
```

#### `budgets remove`

*write token · needs --yes*

Removes a budget, with the same month rules as setting one. The months before keep it, and the history stays.

```text
meowny budgets remove --type <INCOME|EXPENSE> --month <month> [--category-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--type` (required) | one of INCOME, EXPENSE | INCOME or EXPENSE. |
| `--month` (required) | month | The month, like 2026-10. |
| `--category-id` | id | The category. Leave out for the global budget of that side. |

The same month rules as `budgets set`. The change history keeps what the budget was.

This cannot be undone, so it needs `--yes`.

```console
$ meowny budgets remove --type expense --month 2027-01 --category-id 16 --yes
```

With `--json` (one line; shown indented):

```json
{
  "amountLimit": null,
  "categoryId": "16",
  "effectiveFrom": "2027-01-01",
  "id": "16",
  "scope": "CATEGORY",
  "type": "EXPENSE"
}
```

### Recurring items

#### `recurring list`

*read token*

Every recurring item, with its schedule, its next due date and whether it is active.

```text
meowny recurring list
```

```console
$ meowny recurring list

id  name              nextDueDate   amount  categoryId  description                endDate     intervalCount  intervalUnit  isActive  split                                                         startDate
11  Yoga class        2026-08-05     32.00           8  Monthly pass               -                       1  MONTH         true      -                                                             2026-04-05
12  Mobile top-up     2026-09-03         -          10  Whatever the shop charges  -                       1  MONTH         true      -                                                             2026-05-03
 6  Cleaning service  2026-10-11     30.00          10  -                       …
```

With `--json` (one line; shown indented):

```json
[{"amount":"32.00","categoryId":"8","description":"Monthly pass","endDate":null,"id":"11","intervalCount":1,"intervalUnit":"MONTH","isActive":true,"name":"Yoga class","nextDueDate":"2026-08-05","split":null,"startDate":"2026-04-05"},{"amount":null,"categoryId":"10","description":"Whatever the shop charges","endDate":null,"id":"12","intervalCount":1,"intervalUnit":"MONTH","isActive":true,"name":"Mobile top-up","nextDueDate":"2026-09-03","split":null,"startDate":"2026-05-03"},{"amount":"30.00","categoryId":"10","description":null,"endDate":null,"id":"6","intervalCount":2,"intervalUnit":"WEEK","isActive":true,"name":"Cleaning service","nextDueDate":"2026-10-11","split":null,"startDate":"2026-04…
```

#### `recurring dues`

*read token*

Active recurring items that are overdue (due before today) and those due in the next days. Today is the date in the app's time zone.

```text
meowny recurring dues [--within-days <number>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--within-days` | number | How many days ahead count as upcoming, 0 to 366. Default 30. |

```console
$ meowny recurring dues --within-days 10

overdue
id  name              nextDueDate  amount  categoryId  description                endDate  intervalCount  intervalUnit  isActive  split  startDate
11  Yoga class        2026-08-05    32.00           8  Monthly pass               -                    1  MONTH         true      -      2026-04-05
12  Mobile top-up     2026-09-03        -          10  Whatever the shop charges  -                    1  MONTH         true      -      2026-05-03
 6  Cleaning service  2026-10-11    30.00          10  -                          -                    2  WEEK          true      -      2026-04-26

upcoming
id  name         nextDueDate   amount  categoryId  description            endDate  interval…
```

With `--json` (one line; shown indented):

```json
{"overdue":[{"amount":"32.00","categoryId":"8","description":"Monthly pass","endDate":null,"id":"11","intervalCount":1,"intervalUnit":"MONTH","isActive":true,"name":"Yoga class","nextDueDate":"2026-08-05","split":null,"startDate":"2026-04-05"},{"amount":null,"categoryId":"10","description":"Whatever the shop charges","endDate":null,"id":"12","intervalCount":1,"intervalUnit":"MONTH","isActive":true,"name":"Mobile top-up","nextDueDate":"2026-09-03","split":null,"startDate":"2026-05-03"},{"amount":"30.00","categoryId":"10","description":null,"endDate":null,"id":"6","intervalCount":2,"intervalUnit":"WEEK","isActive":true,"name":"Cleaning service","nextDueDate":"2026-10-11","split":null,"startDat…
```

#### `recurring create`

*write token*

Adds a recurring item: a bill or an income that comes back every so many days, weeks, months or years.

```text
meowny recurring create --name <text> [--amount <amount>] --category-id <id> --start-date <date> --interval-unit <DAY|WEEK|MONTH|YEAR> --interval-count <number> [--end-date <date>] [--description <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--name` (required) | text | What it is, 1 to 50 characters. |
| `--amount` | amount | The planned amount. Leave out for a bill that changes every time. |
| `--category-id` (required) | id | The category. |
| `--start-date` (required) | date | The first due date. Later dates are worked out from it. |
| `--interval-unit` (required) | one of DAY, WEEK, MONTH, YEAR | DAY, WEEK, MONTH or YEAR. |
| `--interval-count` (required) | number | Every how many units, 1 to 1000. |
| `--end-date` | date | The last day it can fall due. |
| `--description` | text | A note. |

Leave out `--amount` for a bill that changes every time (electricity, a phone bill). Paying it then needs `--amount`.

```console
$ meowny recurring create --name "Vet plan" --amount 20.00 --category-id 16 --start-date 2026-10-20 --interval-unit month --interval-count 1
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": true,
  "name": "Vet plan",
  "nextDueDate": "2026-10-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

#### `recurring update`

*write token*

Changes a recurring item. Fields left out stay as they are; null clears the end date or the description, and a null amount makes it a bill with no fixed amount.

```text
meowny recurring update <id> [--name <text>] [--amount <amount>] [--category-id <id>] [--start-date <date>] [--interval-unit <DAY|WEEK|MONTH|YEAR>] [--interval-count <number>] [--end-date <date>] [--description <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |
| `--name` | text | The new name. |
| `--amount` | amount | The new planned amount, or null for no fixed amount. |
| `--clear-amount` | no value | Clears amount. |
| `--category-id` | id | The new category. |
| `--start-date` | date | The new first due date. |
| `--interval-unit` | one of DAY, WEEK, MONTH, YEAR | DAY, WEEK, MONTH or YEAR. |
| `--interval-count` | number | Every how many units, 1 to 1000. |
| `--end-date` | date | The new last day, or null for none. |
| `--clear-end-date` | no value | Clears `--end-date`. |
| `--description` | text | The new note, or null to clear it. |
| `--clear-description` | no value | Clears description. |

`--clear-amount` makes it a bill with no fixed amount; `--clear-end-date` and `--clear-description` remove those.

The id can be given as a plain word, as in `meowny recurring update 42`, or with `--id`.

```console
$ meowny recurring update 13 --name "Vet plan plus" --clear-end-date
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": true,
  "name": "Vet plan plus",
  "nextDueDate": "2026-10-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

#### `recurring delete`

*write token · needs --yes*

Deletes a recurring item. The transactions it already made stay.

```text
meowny recurring delete <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |

The id can be given as a plain word, as in `meowny recurring delete 42`, or with `--id`.

This cannot be undone, so it needs `--yes`.

```console
$ meowny recurring delete 13 --yes

done.
```

#### `recurring pay`

*write token*

Turns the current occurrence into a transaction and moves the item to the next one. A bill with no fixed amount needs the amount.

```text
meowny recurring pay <id> [--amount <amount>] [--paid-on <date>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |
| `--amount` | amount | What was paid. Defaults to the planned amount. |
| `--paid-on` | date | The day it was paid. Defaults to today in the app's time zone. |

A bill with no fixed amount needs `--amount`. The next due date moves on by itself.

The id can be given as a plain word, as in `meowny recurring pay 42`, or with `--id`.

```console
$ meowny recurring pay 13 --paid-on 2026-10-14
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "clientUuid": null,
  "description": null,
  "id": "426",
  "myShare": "20.00",
  "name": "Vet plan plus",
  "paymentDate": "2026-10-14",
  "split": []
}
```

#### `recurring skip`

*write token*

Passes over the current occurrence without making a transaction.

```text
meowny recurring skip <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |

The id can be given as a plain word, as in `meowny recurring skip 42`, or with `--id`.

```console
$ meowny recurring skip 13
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": true,
  "name": "Vet plan plus",
  "nextDueDate": "2026-12-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

#### `recurring pause`

*write token*

Pauses a recurring item: it stops coming due until it is resumed.

```text
meowny recurring pause <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |

The id can be given as a plain word, as in `meowny recurring pause 42`, or with `--id`.

```console
$ meowny recurring pause 13
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": false,
  "name": "Vet plan plus",
  "nextDueDate": "2026-12-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

#### `recurring resume`

*write token*

Resumes a paused recurring item, unless its next date is past its end date.

```text
meowny recurring resume <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |

The id can be given as a plain word, as in `meowny recurring resume 42`, or with `--id`.

```console
$ meowny recurring resume 13
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": true,
  "name": "Vet plan plus",
  "nextDueDate": "2026-12-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

#### `recurring end`

*write token · needs --yes*

Ends a recurring item for good: it becomes inactive and its end date is today. What it already paid stays.

```text
meowny recurring end <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |

The id can be given as a plain word, as in `meowny recurring end 42`, or with `--id`.

This cannot be undone, so it needs `--yes`.

```console
$ meowny recurring end 13 --yes
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": "2026-10-20",
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": false,
  "name": "Vet plan plus",
  "nextDueDate": "2026-12-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

#### `recurring split`

*write token*

Splits every payment of a recurring item the same way, from now on. Kept as proportions, so a payment that differs from the plan is shared in the same proportions. EXACT needs an item with an amount that the values add up to. Payments already made are not changed.

```text
meowny recurring split <id> --mode <EQUAL|PERCENT|SHARES|EXACT> [--mine <text>] [--people <id,id,...>] [--guests <text>] [--values <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |
| `--mode` (required) | one of EQUAL, PERCENT, SHARES, EXACT | How the numbers are typed: EQUAL (everyone the same), PERCENT (two decimals, 100.00 in all), SHARES (whole numbers) or EXACT (amounts that add up to the bill). |
| `--mine` | text | Your own value, typed the way the mode says. Leave out for an equal split, or to take the rest in PERCENT and EXACT. |
| `--people` | id,id,... | The friends the bill is shared with, as ids. |
| `--guests` | text | People who are not tracked, by name, separated by commas. Their part leaves your cost and makes no debt. |
| `--values` | text | The value of each friend, in the order of --people, then of each guest, separated by commas. Not for an equal split. |

Splits every payment of a recurring item the same way from now on, kept as proportions: a payment that differs from the plan is shared in the same proportions. `exact` needs an item with an amount that the values add up to. Payments already made keep what they had. An income item cannot be split.

The id can be given as a plain word, as in `meowny recurring split 42`, or with `--id`.

```console
$ meowny recurring split 13 --mode shares --mine 2 --people 5 --values 1
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": true,
  "name": "Vet plan plus",
  "nextDueDate": "2026-10-20",
  "split": {
    "mine": 2,
    "parts": [
      {
        "guest": null,
        "person": "Anna",
        "personId": "5",
        "weight": 1
      }
    ]
  },
  "startDate": "2026-10-20"
}
```

#### `recurring unsplit`

*write token*

Takes the split off a recurring item: its payments are not split from now on. Payments already made keep theirs.

```text
meowny recurring unsplit <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The recurring item. |

The payments are not split from now on. Payments already made keep theirs.

The id can be given as a plain word, as in `meowny recurring unsplit 42`, or with `--id`.

```console
$ meowny recurring unsplit 13
```

With `--json` (one line; shown indented):

```json
{
  "amount": "20.00",
  "categoryId": "16",
  "description": null,
  "endDate": null,
  "id": "13",
  "intervalCount": 1,
  "intervalUnit": "MONTH",
  "isActive": true,
  "name": "Vet plan plus",
  "nextDueDate": "2026-10-20",
  "split": null,
  "startDate": "2026-10-20"
}
```

### Statistics

#### `stats budget`

*read token*

Budget against actual for a month: each category with its budget, what was spent or earned, what is left and the percent used, and the totals.

```text
meowny stats budget --month <month>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--month` (required) | month | The month, like 2026-10. |

```console
$ meowny stats budget --month 2026-09

month: 2026-09

categories
name                 type     budgeted   actual  remaining  pctUsed  categoryId
Eating out           EXPENSE    200.00   177.00      23.00     88.5           7
Entertainment        EXPENSE         -    52.07          -        -           8
Gifts                EXPENSE         -     0.00          -        -          13
Groceries            EXPENSE    500.00   413.00      87.00     82.6           5
Health               EXPENSE         -    12.34          -        -          11
Rent                 EXPENSE    950.00   950.00       0.00    100.0           4
Subscriptions        EXPENSE     45.00    23.98      21.02     53.3           9
Transport            EXPENSE     …
```

With `--json` (one line; shown indented):

```json
{"categories":[{"actual":"177.00","budgeted":"200.00","categoryId":"7","name":"Eating out","pctUsed":88.5,"remaining":"23.00","type":"EXPENSE"},{"actual":"52.07","budgeted":null,"categoryId":"8","name":"Entertainment","pctUsed":null,"remaining":null,"type":"EXPENSE"},{"actual":"0.00","budgeted":null,"categoryId":"13","name":"Gifts","pctUsed":null,"remaining":null,"type":"EXPENSE"},{"actual":"413.00","budgeted":"500.00","categoryId":"5","name":"Groceries","pctUsed":82.6,"remaining":"87.00","type":"EXPENSE"},{"actual":"12.34","budgeted":null,"categoryId":"11","name":"Health","pctUsed":null,"remaining":null,"type":"EXPENSE"},{"actual":"950.00","budgeted":"950.00","categoryId":"4","name":"Rent",…
```

#### `stats trend`

*read token*

One category month by month, with its budget and actual for each month. At most 120 months.

```text
meowny stats trend --category-id <id> --from <month> --to <month>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--category-id` (required) | id | The category. |
| `--from` (required) | month | The first month, like 2026-01. |
| `--to` (required) | month | The last month, like 2026-10. |

```console
$ meowny stats trend --category-id 5 --from 2026-07 --to 2026-09

categoryId: 5

points
month    budgeted  actual
2026-07    500.00  403.30
2026-08    500.00  315.15
2026-09    500.00  413.00
```

With `--json` (one line; shown indented):

```json
{
  "categoryId": "5",
  "points": [
    {
      "actual": "403.30",
      "budgeted": "500.00",
      "month": "2026-07"
    },
    {
      "actual": "315.15",
      "budgeted": "500.00",
      "month": "2026-08"
    },
    {
      "actual": "413.00",
      "budgeted": "500.00",
      "month": "2026-09"
    }
  ]
}
```

#### `stats year`

*read token*

Income, expense and savings for each month of a year.

```text
meowny stats year --year <year>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--year` (required) | year | The year, like 2026. |

```console
$ meowny stats year --year 2026

month    expense   income   savings  savingsRate
2026-01  1890.68  4715.33   2824.65         59.9
2026-02  2162.65  3200.00   1037.35         32.4
2026-03  2079.23  3520.00   1440.77         40.9
2026-04  1904.86  3214.22   1309.36         40.7
2026-05  2834.37  3240.00    405.63         12.5
2026-06  1955.38  3520.00   1564.62         44.4
2026-07  1961.20  3253.11   1291.91         39.7
2026-08  3992.69  3240.00   -752.69        -23.2
2026-09  1961.64  3690.00   1728.36         46.8
2026-10  1485.45     0.00  -1485.45            -
2026-11     0.00     0.00      0.00            -
2026-12     0.00     0.00      0.00            -
```

With `--json` (one line; shown indented):

```json
[{"expense":"1890.68","income":"4715.33","month":"2026-01","savings":"2824.65","savingsRate":59.9},{"expense":"2162.65","income":"3200.00","month":"2026-02","savings":"1037.35","savingsRate":32.4},{"expense":"2079.23","income":"3520.00","month":"2026-03","savings":"1440.77","savingsRate":40.9},{"expense":"1904.86","income":"3214.22","month":"2026-04","savings":"1309.36","savingsRate":40.7},{"expense":"2834.37","income":"3240.00","month":"2026-05","savings":"405.63","savingsRate":12.5},{"expense":"1955.38","income":"3520.00","month":"2026-06","savings":"1564.62","savingsRate":44.4},{"expense":"1961.20","income":"3253.11","month":"2026-07","savings":"1291.91","savingsRate":39.7},{"expense":"39…
```

#### `stats top`

*read token*

The categories with the most spending, or income, in a month, biggest first, with their share of the month.

```text
meowny stats top --month <month> --type <INCOME|EXPENSE> [--limit <number>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--month` (required) | month | The month, like 2026-10. |
| `--type` (required) | one of INCOME, EXPENSE | INCOME or EXPENSE. |
| `--limit` | number | How many categories, 1 to 50. Default 5. |

```console
$ meowny stats top --month 2026-09 --type expense --limit 3

name       actual  categoryId  share
Rent       950.00           4   48.4
Groceries  413.00           5   21.1
Utilities  180.29          10    9.2
```

With `--json` (one line; shown indented):

```json
[
  {
    "actual": "950.00",
    "categoryId": "4",
    "name": "Rent",
    "share": 48.4
  },
  {
    "actual": "413.00",
    "categoryId": "5",
    "name": "Groceries",
    "share": 21.1
  },
  {
    "actual": "180.29",
    "categoryId": "10",
    "name": "Utilities",
    "share": 9.2
  }
]
```

#### `stats commitments`

*read token*

What the active recurring items cost, or bring in, in an average month. Items with no fixed amount are listed but not counted.

```text
meowny stats commitments
```

```console
$ meowny stats commitments

expense: 1148.98
income: 3240.00

items
id  name              type     perMonth
 6  Cleaning service  EXPENSE     65.00
 4  Electricity       EXPENSE         -
 5  Home insurance    EXPENSE     26.00
12  Mobile top-up     EXPENSE         -
 2  Netflix           EXPENSE     12.99
 1  Rent              EXPENSE    950.00
 3  Spotify           EXPENSE     10.99
10  Veg box           EXPENSE     52.00
11  Yoga class        EXPENSE     32.00
 7  Salary            INCOME    3240.00
```

With `--json` (one line; shown indented):

```json
{"expense":"1148.98","income":"3240.00","items":[{"id":"6","name":"Cleaning service","perMonth":"65.00","type":"EXPENSE"},{"id":"4","name":"Electricity","perMonth":null,"type":"EXPENSE"},{"id":"5","name":"Home insurance","perMonth":"26.00","type":"EXPENSE"},{"id":"12","name":"Mobile top-up","perMonth":null,"type":"EXPENSE"},{"id":"2","name":"Netflix","perMonth":"12.99","type":"EXPENSE"},{"id":"1","name":"Rent","perMonth":"950.00","type":"EXPENSE"},{"id":"3","name":"Spotify","perMonth":"10.99","type":"EXPENSE"},{"id":"10","name":"Veg box","perMonth":"52.00","type":"EXPENSE"},{"id":"11","name":"Yoga class","perMonth":"32.00","type":"EXPENSE"},{"id":"7","name":"Salary","perMonth":"3240.00","typ…
```

#### `stats planned`

*read token*

Planned against paid amounts of the recurring items that were paid in a month.

```text
meowny stats planned --month <month>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--month` (required) | month | The month, like 2026-10. |

```console
$ meowny stats planned --month 2026-09

name              difference     paid  paidCount  planned  recurringId
Cleaning service        5.00    65.00          2    60.00            6
Electricity                -    80.30          1        -            4
Netflix                 0.00    12.99          1    12.99            2
Rent                    0.00   950.00          1   950.00            1
Salary                  0.00  3240.00          1  3240.00            7
Spotify                 0.00    10.99          1    10.99            3
Veg box                 0.00    96.00          4    96.00           10
```

With `--json` (one line; shown indented):

```json
[{"difference":"5.00","name":"Cleaning service","paid":"65.00","paidCount":2,"planned":"60.00","recurringId":"6"},{"difference":null,"name":"Electricity","paid":"80.30","paidCount":1,"planned":null,"recurringId":"4"},{"difference":"0.00","name":"Netflix","paid":"12.99","paidCount":1,"planned":"12.99","recurringId":"2"},{"difference":"0.00","name":"Rent","paid":"950.00","paidCount":1,"planned":"950.00","recurringId":"1"},{"difference":"0.00","name":"Salary","paid":"3240.00","paidCount":1,"planned":"3240.00","recurringId":"7"},{"difference":"0.00","name":"Spotify","paid":"10.99","paidCount":1,"planned":"10.99","recurringId":"3"},{"difference":"0.00","name":"Veg box","paid":"96.00","paidCount":…
```

### Export

#### `export preview`

*read token*

What an export of this selection would contain: how many transactions, their dates, the income and expense totals, and the other records (the friends, the history of debts, the overdue payments, the documents).

```text
meowny export preview [--from <date>] [--to <date>] [--category-ids <id,id,...>] [--include-debts <boolean>] [--kind <INCOME|EXPENSE>] [--min-amount <amount>] [--max-amount <amount>] [--text <text>] [--split <split|not_split>] [--sections <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--from` | date | Only transactions on or after this date. |
| `--to` | date | Only transactions on or before this date. |
| `--category-ids` | id,id,... | Only these categories and what belongs to them. Leave out for all of them. |
| `--include-debts` | boolean | With dates or categories chosen, also include the ledger of money lent and borrowed (the friends and their parts of the bills in the selection are in it anyway). Without a choice everything is in. |
| `--kind` | one of INCOME, EXPENSE | Only INCOME or only EXPENSE. Leave out for both. |
| `--min-amount` | amount | Only transactions that cost you at least this (a bill shared with friends counts for your part). |
| `--max-amount` | amount | Only transactions that cost you at most this (a bill shared with friends counts for your part). |
| `--text` | text | Only transactions with this text in the name or the description, whatever the case. |
| `--split` | one of split, not_split | Only the bills shared with friends (split) or only those that are not (not_split). Leave out for both. |
| `--sections` | text | The parts of the file, separated by commas, from: transactions, categories, budgets, recurring, documents, people (the friends and what they owe), due (the overdue recurring payments), ledger (the history of debts) and history (the change history, only in a file with no limits). Leave out for all of them. |

Nothing is written and nothing is asked: this only says how many transactions an export with these choices would contain, and how many of the other records (friends, the history of debts, the overdue payments, documents). Use it to check a selection before exporting. It takes the same choices as `export json`.

```console
$ meowny export preview --from 2026-01-01 --to 2026-09-30

budgets: 14
categories: 15
debtHistory: 24
documents: 0
duePayments: 6
expense: 20742.70
firstDate: 2026-01-01
history: 0
income: 31592.66
lastDate: 2026-09-29
ledger: 0
people: 1
recurring: 12
splitBills: 10
transactions: 257
```

With `--json` (one line; shown indented):

```json
{
  "budgets": 14,
  "categories": 15,
  "debtHistory": 24,
  "documents": 0,
  "duePayments": 6,
  "expense": "20742.70",
  "firstDate": "2026-01-01",
  "history": 0,
  "income": "31592.66",
  "lastDate": "2026-09-29",
  "ledger": 0,
  "people": 1,
  "recurring": 12,
  "splitBills": 10,
  "transactions": 257
}
```

#### `export json`

*read token · asks you in the app*

Writes the data as a JSON file, after the person allows it in the app: the transactions, categories, budgets, recurring items, documents, and the debts (friends, the overdue recurring payments, the history of debts), or the parts and the limits chosen. An existing file is never replaced.

```text
meowny export json --out <path> [--from <date>] [--to <date>] [--category-ids <id,id,...>] [--include-debts <boolean>] [--kind <INCOME|EXPENSE>] [--min-amount <amount>] [--max-amount <amount>] [--text <text>] [--split <split|not_split>] [--sections <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--out` (required) | path | Where to write the file: an absolute path ending in .json, in a folder that exists, that is not there yet. |
| `--from` | date | Only transactions on or after this date. |
| `--to` | date | Only transactions on or before this date. |
| `--category-ids` | id,id,... | Only these categories and what belongs to them. Leave out for all of them. |
| `--include-debts` | boolean | With dates or categories chosen, also include the ledger of money lent and borrowed (the friends and their parts of the bills in the selection are in it anyway). Without a choice everything is in. |
| `--kind` | one of INCOME, EXPENSE | Only INCOME or only EXPENSE. Leave out for both. |
| `--min-amount` | amount | Only transactions that cost you at least this (a bill shared with friends counts for your part). |
| `--max-amount` | amount | Only transactions that cost you at most this (a bill shared with friends counts for your part). |
| `--text` | text | Only transactions with this text in the name or the description, whatever the case. |
| `--split` | one of split, not_split | Only the bills shared with friends (split) or only those that are not (not_split). Leave out for both. |
| `--sections` | text | The parts of the file, separated by commas, from: transactions, categories, budgets, recurring, documents, people (the friends and what they owe), due (the overdue recurring payments), ledger (the history of debts) and history (the change history, only in a file with no limits). Leave out for all of them. |

The whole data as one file, including the change history when nothing limits it. Documents are listed by name, date, type, size, hash and what they are attached to, never their files. Friends, the overdue recurring payments (`duePayments`), the history of debts (`debtHistory`) and the parts of shared bills are in it; each transaction has `amount` (what was paid) and `myShare` (what it cost you). The limits: dates, categories, `--kind` (income or expense), `--min-amount` and `--max-amount` (what it cost you), `--text` (in the name or description) and `--split` (only the bills shared with friends, or only those not). `--sections` names the parts to keep, separated by commas: transactions, categories, budgets, recurring, documents, people, due, ledger, history. With a limit the history of debts is left out unless you give `--include-debts` or name `ledger`. It is not encrypted.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ meowny export json --out /home/you/debts.json --sections people,due,ledger

bytes: 21356
path: /home/you/debts.json
```

#### `export csv`

*read token · asks you in the app*

Writes the transactions as a CSV file, after the person allows it in the app, limited by the dates, the categories, the kind, what they cost you, a text and whether they are shared. An existing file is never replaced.

```text
meowny export csv --out <path> [--from <date>] [--to <date>] [--category-ids <id,id,...>] [--kind <INCOME|EXPENSE>] [--min-amount <amount>] [--max-amount <amount>] [--text <text>] [--split <split|not_split>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--out` (required) | path | Where to write the file: an absolute path ending in .csv, in a folder that exists, that is not there yet. |
| `--from` | date | Only transactions on or after this date. |
| `--to` | date | Only transactions on or before this date. |
| `--category-ids` | id,id,... | Only these categories and what belongs to them. Leave out for all of them. |
| `--kind` | one of INCOME, EXPENSE | Only INCOME or only EXPENSE. Leave out for both. |
| `--min-amount` | amount | Only transactions that cost you at least this (a bill shared with friends counts for your part). |
| `--max-amount` | amount | Only transactions that cost you at most this (a bill shared with friends counts for your part). |
| `--text` | text | Only transactions with this text in the name or the description, whatever the case. |
| `--split` | one of split, not_split | Only the bills shared with friends (split) or only those that are not (not_split). Leave out for both. |

One row per transaction, with a byte order mark so Excel reads the accents, a column with the names of the documents attached to it, and a last column `my_share` with what it cost you (the same as `amount` unless the bill is shared). It takes the limits of `export json` (dates, categories, `--kind`, `--min-amount`, `--max-amount`, `--text`, `--split`) but no parts. Text that starts like a formula is written as text, so opening the file cannot run anything.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ meowny export csv --out /home/you/budget-2026.csv --from 2026-01-01 --kind expense --min-amount 5.00

bytes: 17589
path: /home/you/budget-2026.csv
```

#### `export xlsx`

*read token · asks you in the app*

Writes a workbook (.xlsx) with sheets for transactions, categories, budgets, recurring items, documents, the debts (friends, due payments, history of debts) and a monthly summary, after the person allows it in the app. Dates are ISO and amounts plain numbers. An existing file is never replaced.

```text
meowny export xlsx --out <path> [--from <date>] [--to <date>] [--category-ids <id,id,...>] [--include-debts <boolean>] [--kind <INCOME|EXPENSE>] [--min-amount <amount>] [--max-amount <amount>] [--text <text>] [--split <split|not_split>] [--sections <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--out` (required) | path | Where to write the file: an absolute path ending in .xlsx, in a folder that exists, that is not there yet. |
| `--from` | date | Only transactions on or after this date. |
| `--to` | date | Only transactions on or before this date. |
| `--category-ids` | id,id,... | Only these categories and what belongs to them. Leave out for all of them. |
| `--include-debts` | boolean | With dates or categories chosen, also include the ledger of money lent and borrowed (the friends and their parts of the bills in the selection are in it anyway). Without a choice everything is in. |
| `--kind` | one of INCOME, EXPENSE | Only INCOME or only EXPENSE. Leave out for both. |
| `--min-amount` | amount | Only transactions that cost you at least this (a bill shared with friends counts for your part). |
| `--max-amount` | amount | Only transactions that cost you at most this (a bill shared with friends counts for your part). |
| `--text` | text | Only transactions with this text in the name or the description, whatever the case. |
| `--split` | one of split, not_split | Only the bills shared with friends (split) or only those that are not (not_split). Leave out for both. |
| `--sections` | text | The parts of the file, separated by commas, from: transactions, categories, budgets, recurring, documents, people (the friends and what they owe), due (the overdue recurring payments), ledger (the history of debts) and history (the change history, only in a file with no limits). Leave out for all of them. |

Sheets for transactions (with what each cost you), categories, budgets, recurring items, a monthly summary, the documents (listed, never the files), the friends, the due payments, the history of debts and the parts of shared bills. It takes the limits and `--sections` of `export json` (the change history has no sheet; a workbook of nothing but parts with no sheet is refused). Dates are real dates and amounts are numbers.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ meowny export xlsx --out /home/you/budget.xlsx

bytes: 30656
path: /home/you/budget.xlsx
```

#### `export pdf`

*read token · asks you in the app*

Writes a PDF report for whole months, in English, after the person allows it in the app: totals, charts and budget against actual, and the transactions if asked. An existing file is never replaced.

```text
meowny export pdf --out <path> --from-month <month> --to-month <month> [--category-ids <id,id,...>] [--include-transactions <boolean>] [--show-descriptions <boolean>] [--page <A4|LETTER>] [--include-debts <boolean>] [--include-due-payments <boolean>] [--include-debt-history <boolean>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--out` (required) | path | Where to write the file: an absolute path ending in .pdf, in a folder that exists, that is not there yet. |
| `--from-month` (required) | month | The first month, like 2026-01. |
| `--to-month` (required) | month | The last month, like 2026-10. At most 120 months. |
| `--category-ids` | id,id,... | Only these categories and what belongs to them. Leave out for all of them. |
| `--include-transactions` | boolean | Also list the transactions. Up to 20,000. |
| `--show-descriptions` | boolean | Show each transaction's description. Leave off to share the report without private details. |
| `--page` | one of A4, LETTER | A4 (the default) or LETTER. |
| `--include-debts` | boolean | Also list who owes what on the last day of each month: one line for each friend. |
| `--include-due-payments` | boolean | Also list the recurring payments that are overdue today, with the day each fell due and a total. |
| `--include-debt-history` | boolean | Also list every line of the debts of every friend in the months, with the balance after it, and the recurring payments paid late or overdue. |

A printable report of whole months, in English from the command line (the app writes it in your language). Leave out `--show-descriptions` to share it without private details. `--include-debts` adds who owes what on the last day of each month, `--include-due-payments` the recurring payments that are overdue today, and `--include-debt-history` every line of the debts in those months, with the recurring payments paid late or overdue; these hold the names of your friends and amounts of money.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ meowny export pdf --out /home/you/report.pdf --from-month 2026-01 --to-month 2026-03 --include-transactions --include-debts

bytes: 45159
path: /home/you/report.pdf
```

#### `export debts`

*read token · asks you in the app*

Writes who owes what to a text, CSV or PDF file, in English, after the person allows it in the app: the SUMMARY of all friends, the STATEMENT of one friend (`--person-id`), or the LEDGER of every entry as CSV. An existing file is never replaced.

```text
meowny export debts --out <path> [--what <SUMMARY|STATEMENT|LEDGER>] [--format <TEXT|CSV|PDF>] [--person-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--out` (required) | path | Where to write the file: an absolute path ending in .txt (TEXT), .csv (CSV) or .pdf (PDF), in a folder that exists, that is not there yet. |
| `--what` | one of SUMMARY, STATEMENT, LEDGER | SUMMARY (the default), STATEMENT or LEDGER. |
| `--format` | one of TEXT, CSV, PDF | TEXT (the default), CSV or PDF. LEDGER is CSV only. |
| `--person-id` | id | The friend, for a STATEMENT. |

Saves who owes what to a file you can send yourself; the app sends it to nobody. `--what` is `summary` (all friends), `statement` (one friend, with `--person-id`) or `ledger` (every entry, CSV only). `--format` is `text`, `csv` or `pdf` (the words are in English; the app writes them in your language). It holds the names of your friends and amounts of money, and is not encrypted. The person allows it in the app first.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ meowny export debts --out /home/you/who-owes-what.txt

bytes: 195
path: /home/you/who-owes-what.txt
```

### Backup

#### `backup schedule`

*read token*

The scheduled backup, if one is set up: the folder, how often and how the last one went, or null. Its passphrase is never shown.

```text
meowny backup schedule
```

Shows the scheduled backup the app makes (folder, how often, how the last one went). Setting it up is done in the app.

```console
$ meowny backup schedule

there is nothing to show.
```

With `--json` (one line; shown indented):

```json
null
```

#### `backup create`

*read token · asks you in the app*

Writes an encrypted backup of all the data, after the person allows it in the app. It opens only with the passphrase given here, and only the app can restore it. An existing file is never replaced.

```text
meowny backup create --out <path> --passphrase <text>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--out` (required) | path | Where to write the file: an absolute path ending in .meowbak, in a folder that exists, that is not there yet. |
| `--passphrase` (required) | text | Locks the backup. At least 8 characters. Keep it: without it the backup cannot be opened. |

Everything in one encrypted file, except the files of the documents: their names, dates and links are in it, the files are not. A backup made in the app can include them. Give `--passphrase -` to read the passphrase from standard input so it does not show in the list of running programs. Keep it: without it the backup cannot be opened.

Restoring a backup is done in the app, never with a token.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ echo "correct horse battery staple" | meowny backup create --out /home/you/meowny.meowbak --passphrase -

bytes: 206446
path: /home/you/meowny.meowbak

waiting for you to allow this in the meowNY app...
```

### Documents

#### `documents list`

*read token*

Documents, newest paper first, with the total that match: names, dates, types, sizes and hashes, never the files. Up to 100 a page, 50 unless limit says otherwise.

```text
meowny documents list [--q <text>] [--kind <image|pdf|other>] [--from <date>] [--to <date>] [--transaction-id <id>] [--recurring-id <id>] [--ledger-entry-id <id>] [--limit <number>] [--offset <number>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--q` | text | Only documents whose name contains this text, ignoring case. |
| `--kind` | one of image, pdf, other | Only images, only PDF files, or only other files. |
| `--from` | date | Only documents dated on or after this date (the day of the paper). |
| `--to` | date | Only documents dated on or before this date. |
| `--transaction-id` | id | A transaction. |
| `--recurring-id` | id | A recurring item. |
| `--ledger-entry-id` | id | An entry of the ledger of money lent and borrowed. |
| `--limit` | number | How many to return, 1 to 100. |
| `--offset` | number | How many to skip. |

Names, dates, types, sizes and hashes of the documents, newest paper first, never the files. `--transaction-id`, `--recurring-id` and `--ledger-entry-id` show what is attached to one thing. A read-only token can do this.

```console
$ meowny documents list --q boiler

limit: 50
offset: 0
total: 1

documents
id  name                createdAt                 documentDate  hasFile  keepOnPhone  linkCount  mediaType        sha256                                                        sizeBytes
 1  boiler-receipt.pdf  2026-10-18T09:46:38.898Z  2026-10-01    true     false                0  application/pdf  735a7713978716d1e65eb6bf069e2f410eb72ec5b285861be417aeca0fe…         45
```

With `--json` (one line; shown indented):

```json
{
  "documents": [
    {
      "createdAt": "2026-10-18T09:46:38.898Z",
      "documentDate": "2026-10-01",
      "hasFile": true,
      "id": "1",
      "keepOnPhone": false,
      "linkCount": 0,
      "mediaType": "application/pdf",
      "name": "boiler-receipt.pdf",
      "sha256": "735a7713978716d1e65eb6bf069e2f410eb72ec5b285861be417aeca0feeb935",
      "sizeBytes": 45
    }
  ],
  "limit": 50,
  "offset": 0,
  "total": 1
}
```

#### `documents show`

*read token*

One document: its name, date, type, size and hash, whether this computer holds its file, and in `links` what it is attached to (a debt entry also gives `personId`, the friend). Never the file.

```text
meowny documents show <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The document. |

One document, and the transactions and recurring items it is attached to. Never the file. `has_file` says whether this computer holds the file. A read-only token can do this.

The id can be given as a plain word, as in `meowny documents show 42`, or with `--id`.

```console
$ meowny documents show 1

createdAt: 2026-10-18T09:46:38.898Z
documentDate: 2026-10-01
hasFile: true
id: 1
keepOnPhone: false
linkCount: 0
mediaType: application/pdf
name: boiler-receipt.pdf
sha256: 735a7713978716d1e65eb6bf069e2f410eb72ec5b285861be417aeca0feeb935
sizeBytes: 45

links
(none)
```

With `--json` (one line; shown indented):

```json
{
  "createdAt": "2026-10-18T09:46:38.898Z",
  "documentDate": "2026-10-01",
  "hasFile": true,
  "id": "1",
  "keepOnPhone": false,
  "linkCount": 0,
  "links": [],
  "mediaType": "application/pdf",
  "name": "boiler-receipt.pdf",
  "sha256": "735a7713978716d1e65eb6bf069e2f410eb72ec5b285861be417aeca0feeb935",
  "sizeBytes": 45
}
```

#### `documents add`

*write token*

Stores a file as a document, encrypted, and attaches it where asked. The answer is the document and `alreadyThere`: the same file added again is not stored twice, and the attachments are added to the document that is there. At most 25 MiB.

```text
meowny documents add <file> [--name <text>] [--date <date>] [--keep-on-phone <boolean>] [--transaction-ids <id,id,...>] [--recurring-ids <id,id,...>] [--ledger-entry-ids <id,id,...>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--file` (required) | path | The bytes of the file, as base64url text. The command line reads the file and does this. |
| `--name` | text | The name to keep it under, up to 120 characters. The name of the file if left out. |
| `--date` | date | The day of the paper. Today if left out. |
| `--keep-on-phone` | boolean | Keep the file on the phone too. Off if left out. |
| `--transaction-ids` | id,id,... | Attach it to these transactions. |
| `--recurring-ids` | id,id,... | Attach it to these recurring items. |
| `--ledger-entry-ids` | id,id,... | Attach it to these entries of the ledger. |

The program reads the file itself, as you, and sends its bytes to the app, which stores it encrypted. The app never opens a path on a token's behalf. The name is the name of the file unless you give `--name`. A file of at most 25 MiB; the same file added again is not stored twice: the answer says `alreadyThere` and the attachments are added to the document that is there.

`--transaction-ids`, `--recurring-ids` and `--ledger-entry-ids` attach it to what it belongs to, as comma-separated ids. `--keep-on-phone` also keeps the file on the phone after a sync.

The data can be given as a plain word, as in `meowny documents add 42`, or with `--data`.

```console
$ meowny documents add /home/you/boiler-receipt.pdf --date 2026-10-01
```

With `--json` (one line; shown indented):

```json
{
  "alreadyThere": false,
  "createdAt": "2026-10-18T09:46:38.898Z",
  "documentDate": "2026-10-01",
  "hasFile": true,
  "id": "1",
  "keepOnPhone": false,
  "linkCount": 0,
  "mediaType": "application/pdf",
  "name": "boiler-receipt.pdf",
  "sha256": "735a7713978716d1e65eb6bf069e2f410eb72ec5b285861be417aeca0feeb935",
  "sizeBytes": 45
}
```

#### `documents update`

*write token*

Renames a document, changes its date, or switches whether the phone keeps its file. Give at least one. The contents cannot be changed.

```text
meowny documents update <id> [--name <text>] [--date <date>] [--keep-on-phone <boolean>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The document. |
| `--name` | text | The new name, up to 120 characters. |
| `--date` | date | The new day of the paper. |
| `--keep-on-phone` | boolean | Whether the phone keeps the file. |

Only the flags you give change. The contents of a document cannot be changed: add the new file and delete the old one.

The id can be given as a plain word, as in `meowny documents update 42`, or with `--id`.

```console
$ meowny documents update 1 --name "Boiler repair" --keep-on-phone
```

With `--json` (one line; shown indented):

```json
{
  "createdAt": "2026-10-18T09:46:38.898Z",
  "documentDate": "2026-10-01",
  "hasFile": true,
  "id": "1",
  "keepOnPhone": true,
  "linkCount": 0,
  "mediaType": "application/pdf",
  "name": "Boiler repair",
  "sha256": "735a7713978716d1e65eb6bf069e2f410eb72ec5b285861be417aeca0feeb935",
  "sizeBytes": 45
}
```

#### `documents delete`

*write token · needs --yes*

Deletes a document, its file and its attachments. The transactions it was attached to stay.

```text
meowny documents delete <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The document. |

Deletes the document, its file and its attachments. The transactions it was attached to stay.

The id can be given as a plain word, as in `meowny documents delete 42`, or with `--id`.

This cannot be undone, so it needs `--yes`.

```console
$ meowny documents delete 1 --yes

done.
```

#### `documents link`

*write token*

Attaches a document to a transaction, a recurring item or a ledger entry. Giving one that is attached already changes nothing.

```text
meowny documents link <id> [--transaction-id <id>] [--recurring-id <id>] [--ledger-entry-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The document. |
| `--transaction-id` | id | A transaction. |
| `--recurring-id` | id | A recurring item. |
| `--ledger-entry-id` | id | An entry of the ledger of money lent and borrowed. |

Give exactly one of `--transaction-id`, `--recurring-id` and `--ledger-entry-id` (an entry of the ledger of money lent and borrowed, such as a loan note). Attaching what is attached already changes nothing.

The id can be given as a plain word, as in `meowny documents link 42`, or with `--id`.

```console
$ meowny documents link 1 --transaction-id 424

done.
```

#### `documents unlink`

*write token*

Detaches a document from a transaction, a recurring item or a ledger entry. The document and the transaction stay.

```text
meowny documents unlink <id> [--transaction-id <id>] [--recurring-id <id>] [--ledger-entry-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The document. |
| `--transaction-id` | id | A transaction. |
| `--recurring-id` | id | A recurring item. |
| `--ledger-entry-id` | id | An entry of the ledger of money lent and borrowed. |

Detaches it. The document and the transaction stay.

The id can be given as a plain word, as in `meowny documents unlink 42`, or with `--id`.

```console
$ meowny documents unlink 1 --transaction-id 424

done.
```

#### `documents get`

*write token · asks you in the app*

Writes a copy of a document's file, after the person allows it in the app. It is not encrypted. An existing file is never replaced.

```text
meowny documents get <id> --out <path>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The document. |
| `--out` (required) | path | Where to write the file: an absolute path in a folder that exists, that is not there yet, ending in the extension of the kind of file (.pdf, .png, .jpg, .gif, .webp, .txt, .html, .csv, or .bin for a file of no known kind). |

The only command that gives a file back, so it needs a token that can write **and your yes**: the app shows which token, which document, its size and the exact file, and writes it only if you click Allow. The copy is **not encrypted**.

The file name must end in the extension of the kind of file the app found, never one the caller picks: `.pdf`, `.png`, `.jpg`, `.gif`, `.webp`, `.txt`, `.html`, `.csv`, or `.bin` for a file of no known kind. So a copy cannot be given a name a system would run (`.sh`, `.bat`, `.desktop`). A file that exists is never replaced.

The id can be given as a plain word, as in `meowny documents get 42`, or with `--id`.

The app asks you to allow this each time, in a dialog; nothing is written until you do. See [Files need your yes](#files).

```console
$ meowny documents get 1 --out /home/you/boiler-receipt-copy.pdf

bytes: 45
path: /home/you/boiler-receipt-copy.pdf
```

### Friends

#### `people list`

*read token*

The friends with what each owes you (negative: you owe them), the largest first. Archived friends only if asked.

```text
meowny people list [--archived <boolean>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--archived` | boolean | Also list the friends that were archived. |

Your friends with what each owes you, the largest first. A friend is a name and nothing else: no phone number, email or address is kept. A balance below zero means you owe them. Archived friends only with `--archived`. A read-only token can do this.

```console
$ meowny people list

id  name   archived  balance  remindEveryDays  remindOn
 1  Marta  false       85.05                -  -
 5  Anna   false       42.00                -  -
 6  Bob    false       30.00                7  2026-11-01
 2  Tomás  false      -23.95                -  -
 3  Priya  false        0.00                -  -
```

With `--json` (one line; shown indented):

```json
[
  {
    "archived": false,
    "balance": "85.05",
    "id": "1",
    "name": "Marta",
    "remindEveryDays": null,
    "remindOn": null
  },
  {
    "archived": false,
    "balance": "42.00",
    "id": "5",
    "name": "Anna",
    "remindEveryDays": null,
    "remindOn": null
  },
  {
    "archived": false,
    "balance": "30.00",
    "id": "6",
    "name": "Bob",
    "remindEveryDays": 7,
    "remindOn": "2026-11-01"
  },
  {
    "archived": false,
    "balance": "-23.95",
    "id": "2",
    "name": "Tomás",
    "remindEveryDays": null,
    "remindOn": null
  },
  {
    "archived": false,
    "balance": "0.00",
    "id": "3",
    "name": "Priya",
    "remindEveryDays": null,
    "remindOn": null
  }
]
```

#### `people show`

*read token*

One friend and their ledger: their parts of shared bills and the entries by date, with the balance after each line.

```text
meowny people show <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The friend. |

A friend and their ledger: their parts of shared bills (`BILL`) and the entries you made, by date, with the balance after each line. The last line is where the balance ends up.

The id can be given as a plain word, as in `meowny people show 42`, or with `--id`.

```console
$ meowny people show 5

person: {"archived":false,"balance":"42.00","id":"5","name":"Anna","remindEveryDays":null,"remindOn":null}

lines
amount  balance  date        effect  entryId  kind  transactionId  what
 30.00    30.00  2026-10-12   30.00        -  BILL            425  Dinner
 12.00    42.00  2026-10-13   12.00       11  LENT              -  taxi
```

With `--json` (one line; shown indented):

```json
{
  "lines": [
    {
      "amount": "30.00",
      "balance": "30.00",
      "date": "2026-10-12",
      "effect": "30.00",
      "entryId": null,
      "kind": "BILL",
      "transactionId": "425",
      "what": "Dinner"
    },
    {
      "amount": "12.00",
      "balance": "42.00",
      "date": "2026-10-13",
      "effect": "12.00",
      "entryId": "11",
      "kind": "LENT",
      "transactionId": null,
      "what": "taxi"
    }
  ],
  "person": {
    "archived": false,
    "balance": "42.00",
    "id": "5",
    "name": "Anna",
    "remindEveryDays": null,
    "remindOn": null
  }
}
```

#### `people create`

*write token*

Adds a friend. A friend is a name, 1 to 50 characters, unique among the friends who are not archived. Nothing else about a person is kept.

```text
meowny people create --name <text>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--name` (required) | text | The name. |

The name is 1 to 50 characters and unique among the friends who are not archived, ignoring case.

```console
$ meowny people create --name Anna
```

With `--json` (one line; shown indented):

```json
{
  "archived": false,
  "balance": "0.00",
  "id": "5",
  "name": "Anna",
  "remindEveryDays": null,
  "remindOn": null
}
```

#### `people update`

*write token*

Renames a friend. Fields left out stay as they are. The reminder fields are kept for data written before reminders became a setting of each device (every so many days after a debt last changed), and nothing rings from them.

```text
meowny people update <id> [--name <text>] [--remind-on <date>] [--remind-every-days <number>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The friend. |
| `--name` | text | The new name. |
| `--remind-on` | date | Not used for reminders any more; kept as it was written. Null clears it. |
| `--clear-remind-on` | no value | Clears `--remind-on`. |
| `--remind-every-days` | number | Not used for reminders any more; kept as it was written, 1 to 365, or null. |
| `--clear-remind-every-days` | no value | Clears `--remind-every-days`. |

Only the flags you give change. The reminder flags (`--remind-on`, `--remind-every-days`, `--clear-remind-on`) are kept for data written before reminders became a setting of each device: nothing rings from them. What a reminder is about can be read with `debts reminders`.

The id can be given as a plain word, as in `meowny people update 42`, or with `--id`.

```console
$ meowny people update 6 --remind-on 2026-11-01 --remind-every-days 7
```

With `--json` (one line; shown indented):

```json
{
  "archived": false,
  "balance": "0.00",
  "id": "6",
  "name": "Bob",
  "remindEveryDays": 7,
  "remindOn": "2026-11-01"
}
```

#### `people archive`

*write token*

Hides a friend from the lists and pickers. Their balance and history stay, and the name can be used again.

```text
meowny people archive <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The friend. |

Hides a friend from the lists. Their balance and history stay, and the name can be used again. A friend is never deleted.

The id can be given as a plain word, as in `meowny people archive 42`, or with `--id`.

```console
$ meowny people archive 6
```

With `--json` (one line; shown indented):

```json
{
  "archived": true,
  "balance": "0.00",
  "id": "6",
  "name": "Bob",
  "remindEveryDays": 7,
  "remindOn": "2026-11-01"
}
```

#### `people unarchive`

*write token*

Brings back an archived friend, if no other friend has taken the name.

```text
meowny people unarchive <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The friend. |

Brings the friend back, if no other friend has taken the name.

The id can be given as a plain word, as in `meowny people unarchive 42`, or with `--id`.

```console
$ meowny people unarchive 6
```

With `--json` (one line; shown indented):

```json
{
  "archived": false,
  "balance": "0.00",
  "id": "6",
  "name": "Bob",
  "remindEveryDays": 7,
  "remindOn": "2026-11-01"
}
```

#### `people settle`

*write token*

Marks a payment between you and a friend: they paid you back if they owe you, you paid them back if you owe them. With no amount it is the whole balance. An amount larger than the debt is accepted: the balance turns around and the answer says how much was over.

```text
meowny people settle <id> [--amount <amount>] [--date <date>] [--note <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The friend. |
| `--amount` | amount | How much was paid, more than zero. The whole balance if left out. |
| `--date` | date | The day it was paid. Today if left out. |
| `--note` | text | A note, up to 200 characters. |

Marks a payment between you and a friend. If they owe you, it is a repayment to you; if you owe them, a repayment by you. Without `--amount` it is the whole balance.

An amount larger than the debt is accepted: the balance turns around and the answer says in `over` how much of the payment was more than the debt. Repayments are never income or expenses.

The id can be given as a plain word, as in `meowny people settle 42`, or with `--id`.

```console
$ meowny people settle 5 --amount 50.00 --date 2026-10-15
```

With `--json` (one line; shown indented):

```json
{
  "after": "-8.00",
  "before": "42.00",
  "entry": {
    "amount": "50.00",
    "date": "2026-10-15",
    "id": "12",
    "kind": "REPAID_TO_ME",
    "note": null,
    "personId": "5",
    "transactionId": null
  },
  "over": "8.00"
}
```

### Debts

#### `debts summary`

*read token*

What friends owe you and what you owe them, in all and for each friend whose balance is not zero. Neither is income or expense.

```text
meowny debts summary
```

What friends owe you and what you owe them, in all and for each friend whose balance is not zero. Neither is income or expense, and neither touches a budget.

```console
$ meowny debts summary

net: 143.10
owedToYou: 167.05
youOwe: 23.95

people
id  name   archived  balance  remindEveryDays  remindOn
 1  Marta  false       85.05                -  -
 5  Anna   false       42.00                -  -
 6  Bob    false       30.00                7  2026-11-01
 2  Tomás  false      -23.95                -  -
 4  Sorin  true        10.00                -  -
```

With `--json` (one line; shown indented):

```json
{
  "net": "143.10",
  "owedToYou": "167.05",
  "people": [
    {
      "archived": false,
      "balance": "85.05",
      "id": "1",
      "name": "Marta",
      "remindEveryDays": null,
      "remindOn": null
    },
    {
      "archived": false,
      "balance": "42.00",
      "id": "5",
      "name": "Anna",
      "remindEveryDays": null,
      "remindOn": null
    },
    {
      "archived": false,
      "balance": "30.00",
      "id": "6",
      "name": "Bob",
      "remindEveryDays": 7,
      "remindOn": "2026-11-01"
    },
    {
      "archived": false,
      "balance": "-23.95",
      "id": "2",
      "name": "Tomás",
      "remindEveryDays": null,
      "remindOn": null
    },
    {
      "archived": true,
      "balance": "10.00",
      "id": "4",
      "name": "Sorin",
      "remindEveryDays": null,
      "remindOn": null
    }
  ],
  "youOwe": "23.95"
}
```

#### `debts due`

*read token*

The recurring payments that are overdue and not paid or skipped, one for each occurrence that was missed (at most 24 for an item), the oldest first, with the day each fell due, what it is for (null for a bill with no fixed amount), the days late and what they come to. A payment is overdue from the day after its day; income and paused or ended items are not listed.

```text
meowny debts due
```

The recurring payments you have not paid or skipped and whose day has gone by: one for each occurrence that was missed (a rent unpaid for three months is three), the oldest first, at most 24 for an item. A payment is overdue from the day after its day. `amount` is empty for a bill that changes every time. Income and paused or ended items are not listed. Paying one is `recurring pay`.

```console
$ meowny debts due

total: 126.00
unknown: 2

payments
name              amount  category       categoryId  daysOverdue  dueDate     recurringId
Yoga class         32.00  Entertainment           8           74  2026-08-05           11
Mobile top-up          -  Utilities              10           45  2026-09-03           12
Yoga class         32.00  Entertainment           8           43  2026-09-05           11
Mobile top-up          -  Utilities              10           15  2026-10-03           12
Yoga class         32.00  Entertainment           8           13  2026-10-05           11
Cleaning service   30.00  Utilities              10            7  2026-10-11            6
```

With `--json` (one line; shown indented):

```json
{"payments":[{"amount":"32.00","category":"Entertainment","categoryId":"8","daysOverdue":74,"dueDate":"2026-08-05","name":"Yoga class","recurringId":"11"},{"amount":null,"category":"Utilities","categoryId":"10","daysOverdue":45,"dueDate":"2026-09-03","name":"Mobile top-up","recurringId":"12"},{"amount":"32.00","category":"Entertainment","categoryId":"8","daysOverdue":43,"dueDate":"2026-09-05","name":"Yoga class","recurringId":"11"},{"amount":null,"category":"Utilities","categoryId":"10","daysOverdue":15,"dueDate":"2026-10-03","name":"Mobile top-up","recurringId":"12"},{"amount":"32.00","category":"Entertainment","categoryId":"8","daysOverdue":13,"dueDate":"2026-10-05","name":"Yoga class","re…
```

#### `debts history`

*read token*

The history of debts, newest first, a page at a time: the lines of every friend (lent, borrowed, paid back, their parts of shared bills, with the balance after each) and the recurring payments that were paid late or are overdue. A payment made on its day or before it is not in it.

```text
meowny debts history [--from <date>] [--to <date>] [--person-id <id>] [--limit <number>] [--offset <number>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--from` | date | Only lines on or after this date. |
| `--to` | date | Only lines on or before this date. |
| `--person-id` | id | Only the lines of this friend (and none of the recurring payments). |
| `--limit` | number | How many to return, 1 to 200. Default 50. |
| `--offset` | number | How many to skip, for the next page. Default 0. |

Everything that changed what you owe or are owed, newest first, a page at a time (`--limit` up to 200, `--offset` for the next page): the lines of every friend with the balance after each, and the recurring payments that were paid **late** or are **overdue**. A payment made on its day or before it is not in it, and one you skipped leaves no line. `--person-id` keeps to one friend (and none of the recurring payments); `--from` and `--to` keep to some days.

```console
$ meowny debts history --limit 5

limit: 5
offset: 0
total: 49

lines
amount  balance  date        dueDate     effect  entryId  friend  kind         personId  recurringId  transactionId  what
 12.00    42.00  2026-10-13  -            12.00       11  Anna    LENT                5            -              -  taxi
 30.00    30.00  2026-10-12  -            30.00        -  Bob     BILL                6            -            425  Dinner
 30.00    30.00  2026-10-12  -            30.00        -  Anna    BILL                5            -            425  Dinner
 12.00    85.05  2026-10-12  -            12.00        -  Marta   BILL                1            -            404  Veg box
 30.00        -  2026-10-11  2026-10-11       -…
showing 1-5 of 49; next page: --offset 5
```

With `--json` (one line; shown indented):

```json
{"limit":5,"lines":[{"amount":"12.00","balance":"42.00","date":"2026-10-13","dueDate":null,"effect":"12.00","entryId":"11","friend":"Anna","kind":"LENT","personId":"5","recurringId":null,"transactionId":null,"what":"taxi"},{"amount":"30.00","balance":"30.00","date":"2026-10-12","dueDate":null,"effect":"30.00","entryId":null,"friend":"Bob","kind":"BILL","personId":"6","recurringId":null,"transactionId":"425","what":"Dinner"},{"amount":"30.00","balance":"30.00","date":"2026-10-12","dueDate":null,"effect":"30.00","entryId":null,"friend":"Anna","kind":"BILL","personId":"5","recurringId":null,"transactionId":"425","what":"Dinner"},{"amount":"12.00","balance":"85.05","date":"2026-10-12","dueDate":…
```

#### `debts reminders`

*read token*

Which friends a reminder is due for today, and the next days with one. Every friend whose balance is not zero is reminded this many days after the day their debt last changed (the latest line of their statement), and again after as many; 0 is none. The app rings them (a device sets its own period and time of day); this only reads what they would be about.

```text
meowny debts reminders --every-days <number>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--every-days` (required) | number | The period in days, 0 to 365. |

Which friends a reminder is due for today and the next days with one. Every friend whose balance is not zero is reminded `--every-days` days after the day their debt last changed (the latest line of their statement), and again after as many; 0 is none. The app rings them, each device with its own period and time of day: this only reads what they would be about, and no notification is sent from the command line.

```console
$ meowny debts reminders --every-days 7

today: 2026-10-18

due
name   dueOn       personId
Sorin  2026-10-17         4
Tomás  2026-10-18         2

upcoming
count  date
    1  2026-10-18
    2  2026-10-19
    1  2026-10-20
    1  2026-10-24
```

With `--json` (one line; shown indented):

```json
{
  "due": [
    {
      "dueOn": "2026-10-17",
      "name": "Sorin",
      "personId": "4"
    },
    {
      "dueOn": "2026-10-18",
      "name": "Tomás",
      "personId": "2"
    }
  ],
  "today": "2026-10-18",
  "upcoming": [
    {
      "count": 1,
      "date": "2026-10-18"
    },
    {
      "count": 2,
      "date": "2026-10-19"
    },
    {
      "count": 1,
      "date": "2026-10-20"
    },
    {
      "count": 1,
      "date": "2026-10-24"
    }
  ]
}
```

### Shared bills

#### `splits preview`

*read token*

What a split would give each person for a bill of this amount, without saving anything: your part, theirs, whether the numbers add up and how much is left.

```text
meowny splits preview --amount <amount> --mode <EQUAL|PERCENT|SHARES|EXACT> [--mine <text>] [--people <id,id,...>] [--guests <text>] [--values <text>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--amount` (required) | amount | The bill, more than zero. |
| `--mode` (required) | one of EQUAL, PERCENT, SHARES, EXACT | How the numbers are typed: EQUAL (everyone the same), PERCENT (two decimals, 100.00 in all), SHARES (whole numbers) or EXACT (amounts that add up to the bill). |
| `--mine` | text | Your own value, typed the way the mode says. Leave out for an equal split, or to take the rest in PERCENT and EXACT. |
| `--people` | id,id,... | The friends the bill is shared with, as ids. |
| `--guests` | text | People who are not tracked, by name, separated by commas. Their part leaves your cost and makes no debt. |
| `--values` | text | The value of each friend, in the order of --people, then of each guest, separated by commas. Not for an equal split. |

Shows what a split would give each person for a bill of this amount, and saves nothing. `--mode` is `equal`, `percent` (two decimals, 100.00 in all), `shares` (whole numbers) or `exact` (amounts that add up to the bill). `--mine` is your value; in `percent` and `exact` leave it out to take the rest.

`--values` has one value for each friend in the order of `--people`, then one for each guest in the order of `--guests`. The parts always add up to the bill to the cent: the odd cents go to the largest remainders, and to you first.

```console
$ meowny splits preview --amount 90.00 --mode percent --people 5,6 --values 30.00,30.00

complete: true
left: 0.00
mine: 36.00

parts
amount  guest  person  personId
 27.00  -      Anna           5
 27.00  -      Bob            6
```

With `--json` (one line; shown indented):

```json
{
  "complete": true,
  "left": "0.00",
  "mine": "36.00",
  "parts": [
    {
      "amount": "27.00",
      "guest": null,
      "person": "Anna",
      "personId": "5"
    },
    {
      "amount": "27.00",
      "guest": null,
      "person": "Bob",
      "personId": "6"
    }
  ]
}
```

### Ledger

#### `ledger add`

*write token*

Records money lent, borrowed or paid back with a friend. It is never income or expense. For a borrowing, give `--expense-name` and `--expense-category-id` to add your share to your expenses in the same step, with the entry linked to it.

```text
meowny ledger add --person-id <id> --kind <LENT|BORROWED|REPAID_TO_ME|REPAID_BY_ME> --amount <amount> --date <date> [--note <text>] [--transaction-id <id>] [--expense-name <text>] [--expense-category-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--person-id` (required) | id | The friend. |
| `--kind` (required) | one of LENT, BORROWED, REPAID_TO_ME, REPAID_BY_ME | LENT (you lent), BORROWED (you borrowed), REPAID_TO_ME (they paid you back) or REPAID_BY_ME (you paid them back). |
| `--amount` (required) | amount | More than zero. |
| `--date` (required) | date | The day it happened. |
| `--note` | text | A note, up to 200 characters. |
| `--transaction-id` | id | A transaction this belongs to. |
| `--expense-name` | text | BORROWED only: the name of the expense that is made for your share (1 to 50 characters). Needs `--expense-category-id`. |
| `--expense-category-id` | id | BORROWED only: the expense category for your share. Needs `--expense-name`. |

Money lent, borrowed or paid back. It is never income or expense and appears in no budget or statistic. `--kind` is `lent`, `borrowed`, `repaid_to_me` or `repaid_by_me`.

For a borrowing, give `--expense-name` and `--expense-category-id` to add your share to your expenses in the same step, with the entry linked to it. Paying them back later is another entry and never a second expense.

```console
$ meowny ledger add --person-id 5 --kind lent --amount 10.00 --date 2026-10-13 --note taxi
```

With `--json` (one line; shown indented):

```json
{
  "amount": "10.00",
  "date": "2026-10-13",
  "id": "11",
  "kind": "LENT",
  "note": "taxi",
  "personId": "5",
  "transactionId": null
}
```

#### `ledger update`

*write token*

Changes an entry of the ledger. Fields left out stay as they are; a null note or `--transaction-id` clears it. The expense it came with is not changed.

```text
meowny ledger update <id> [--person-id <id>] [--kind <LENT|BORROWED|REPAID_TO_ME|REPAID_BY_ME>] [--amount <amount>] [--date <date>] [--note <text>] [--transaction-id <id>]
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The entry. |
| `--person-id` | id | The friend. |
| `--kind` | one of LENT, BORROWED, REPAID_TO_ME, REPAID_BY_ME | LENT, BORROWED, REPAID_TO_ME or REPAID_BY_ME. |
| `--amount` | amount | More than zero. |
| `--date` | date | The day it happened. |
| `--note` | text | The new note, or null to clear it. |
| `--clear-note` | no value | Clears note. |
| `--transaction-id` | id | The transaction it belongs to, or null for none. |
| `--clear-transaction-id` | no value | Clears `--transaction-id`. |

Only the flags you give change. `--clear-note` and `--clear-transaction-id` remove those. The expense an entry came with is not changed.

The id can be given as a plain word, as in `meowny ledger update 42`, or with `--id`.

```console
$ meowny ledger update 11 --amount 12.00
```

With `--json` (one line; shown indented):

```json
{
  "amount": "12.00",
  "date": "2026-10-13",
  "id": "11",
  "kind": "LENT",
  "note": "taxi",
  "personId": "5",
  "transactionId": null
}
```

#### `ledger delete`

*write token · needs --yes*

Deletes an entry of the ledger. The balance of the friend changes by that much. The change history keeps what it said.

```text
meowny ledger delete <id>
```

| Flag | Type | Description |
| --- | --- | --- |
| `--id` (required) | id | The entry. |

The balance of the friend changes by that much. The change history keeps what the entry said.

The id can be given as a plain word, as in `meowny ledger delete 42`, or with `--id`.

This cannot be undone, so it needs `--yes`.

```console
$ meowny ledger delete 11 --yes

done.
```

## Global flags and settings

### Flags every command takes

| Flag | What it does |
| --- | --- |
| `--json` | Print JSON: the data on standard output, an error as {code, message} on standard error. |
| `--yes` | Go ahead with a command that deletes or ends something. |
| `--token-file <path>` | Read the token from this file. It must not be readable by other users. |
| `--data-dir <path>` | The app's data folder, where it writes its port. Default: the app's usual folder. |
| `--port <number>` | Connect to this port instead of reading it from the app's data folder. |
| `--timeout <seconds>` | How long to wait for the app. Default 30. |
| `--app` | For schema: ask the running app for its own list of routes. |
| `--help, -h` | Show help. Also: meowny help <command>. |
| `--version, -V` | Show the version. |

### Environment variables

| Variable | What it is |
| --- | --- |
| `MEOWNY_TOKEN` | The access token, made in the app (Settings, command line access). |
| `MEOWNY_TOKEN_FILE` | A file holding the token. Same as --token-file. |
| `MEOWNY_DATA_DIR` | The app's data folder. Same as --data-dir. |
| `MEOWNY_PORT` | The port to connect to. Same as --port. |

The token is looked for in this order, and the first one found is used: `--token-file`, then `MEOWNY_TOKEN`, then `MEOWNY_TOKEN_FILE`, then the file `meowny login` saved. A token file that other users can read is refused.

### Where things are

|  | Linux | macOS | Windows |
| --- | --- | --- | --- |
| The app's data folder (it writes its port there) | `~/.local/share/org.meowny.app/` | `~/Library/Application Support/org.meowny.app/` | `%APPDATA%\org.meowny.app\` |
| The token `meowny login` saves | `~/.config/meowny/token` | `~/Library/Application Support/meowny/token` | `%APPDATA%\meowny\token` |

## Exit codes and errors

A command ends with one of these exit codes, so a script can tell what happened without reading any text.

| Exit code | Means |
| --- | --- |
| **0** | success |
| **1** | error: the app is not reachable, the answer was not understood, or the request failed for another reason |
| **2** | bad usage: a missing or unknown flag, or a value the app refused |
| **3** | the token is missing, wrong, expired or may not do this, the person said no in the app, or the app is locked |
| **4** | not found |

With `--json`, an error is one line on standard error, `{"code": "...", "message": "..."}`. The `code` is one of these and does not change; the message is for people.

| Code | Exit | What it means and what to do |
| --- | --- | --- |
| `app_unavailable` | 1 | The app is not running, or command line access is off. Start the app, and turn it on in Settings, command line access. |
| `bad_answer` | 1 | Something answered that is not the app. Check that `--port` or the data folder points at meowNY. |
| `bad_token` | 3 | What was given is not a token. A token is `mny_` and 64 letters and digits. Copy it again from the app. |
| `denied` | 3 | You (or nobody, in time) did not allow it in the app. After a No the same token is refused for 30 seconds. |
| `file_exists` | 1 | The file already exists. A file is never replaced; choose another name. |
| `forbidden` | 3 | The token may not do this, usually because it is read only. Make a token that can write. |
| `internal` | 1 | Something went wrong inside the app. Nothing is shown about what, on purpose, because it could quote your data. |
| `invalid_input` | 2 | The app refused a value (an amount with one decimal, a date that does not exist). The message says which. |
| `locked` | 3 | The app is locked. Unlock it in the app and try again. The command line cannot unlock it. |
| `method_not_allowed` | 1 | The request was not a POST. The program always sends one; you only see this when using the HTTP API by hand. |
| `name_exists` | 1 | A category with that name is already there. |
| `no_token` | 3 | No token was given. Set `MEOWNY_TOKEN`, use `--token-file`, or save one with `meowny login`. |
| `not_found` | 4 | The transaction, category or other record is not there (maybe it was already deleted). |
| `not_set_up` | 1 | The app has no data yet. Open it and finish the first-run screen. |
| `timeout` | 1 | The app did not answer in time (30 seconds, or 3 minutes for a command you have to allow). Raise it with `--timeout`. |
| `token_file_unsafe` | 3 | The token file can be read by other users of the computer, so it is not used. Run `chmod 600` on it. |
| `too_large` | 1 | The request is too large (over 1 MiB), or the file is over 25 MiB, the most the app keeps. |
| `unauthorized` | 3 | The app does not accept the token: it is unknown, expired or revoked. Make a new one in Settings. |
| `unknown_route` | 1 | This app has no such command: the program is newer than the app. Update the app. |
| `usage` | 2 | The command line is wrong: a flag is missing or unknown, or a delete has no `--yes`. Nothing was sent. Run the command with `--help`. |

What failing really looks like, each recorded from the program:

*There is no token (exit code 3)*

```
$ meowny status
error: no token: set MEOWNY_TOKEN, give --token-file, or save one with: meowny login  (make a token in the app, Settings, command line access)
```

*The token is not the right form (exit code 3)*

```
$ MEOWNY_TOKEN=hello meowny status
error: the token is not in the right form (mny_ and 64 letters and digits); copy it again from the app
```

*The app does not know the token (exit code 3)*

```
$ meowny status
error: the token is missing, unknown, expired or revoked. Make a new token in the app if this one has expired or was revoked.
```

*A read-only token tries to change something (exit code 3)*

```
$ meowny categories create --name Pets --type expense
error: this token is read-only; make a token that can write in the app
```

*A value the app refuses (exit code 2)*

```
$ meowny transactions create --name Bread --amount 2.4 --payment-date 2026-10-14 --category-id 5
error: `amount` must be an amount as text with two decimals, like "12.50"
```

*A missing flag (exit code 2)*

```
$ meowny transactions create --name Bread
error: missing --amount
```

*Deleting without --yes (exit code 2)*

```
$ meowny transactions delete 5
error: transactions delete cannot be undone; add --yes to go ahead
```

*Something that is not there (exit code 4)*

```
$ meowny transactions delete 999999 --yes
error: not found
```

*A file the program cannot read (exit code 2)*

```
$ meowny documents add /home/you/missing.pdf
error: --file: cannot read "/home/you/missing.pdf": there is no such file
```

*A name for a copy that a system could run (exit code 2)*

```
$ meowny documents get 1 --out /home/you/copy.sh
error: the file name must end in .pdf
```

*A name that is taken (exit code 1)*

```
$ meowny categories create --name Groceries --type expense
error: that name already exists
```

*A command that does not exist (exit code 2)*

```
$ meowny backup restore
error: backup needs one of: schedule, create
```

*The app is locked (exit code 3)*

```
$ meowny categories list
error: the app is locked. Unlock it in the app, then try again.
```

*The app is not running, or command line access is off (exit code 1)*

```
$ meowny status
error: the app is not running, or command line access is off (turn it on in Settings, command line access)
```

*A file that already exists (exit code 1)*

```
$ meowny export csv --out /home/you/taken.csv
error: that file already exists; choose a name that does not
```

*A file name that is not an export (exit code 2)*

```
$ meowny export json --out /home/you/data.txt
error: the file name must end in .json
```

*You click Deny in the app (exit code 3)*

```
$ meowny export json --out /home/you/second.json
error: the person did not allow this in the app
```

*Asking again at once after a Deny (exit code 3)*

```
$ meowny export json --out /home/you/second.json
error: the person said no to this token a moment ago; try again later
```

## For AI agents and scripts

Everything above holds for a script, and a few things make an agent's life easy.

1. Give it a **read-only token** in `MEOWNY_TOKEN`, and a read-and-write token only if it must change things. Tokens expire; ask for a new one when you get exit code 3 with `unauthorized`.
2. Run `meowny schema` once. It prints, offline and as JSON, every command with its flags, types, whether it needs a token that can write, whether it needs `--yes`, and every exit code and error code.
3. Always pass `--json`. Read data from standard output; read errors, as `{code, message}`, from standard error. Branch on the **exit code** (0 done, 1 error, 2 bad usage, 3 token or locked, 4 not found).
4. Create a transaction with a fixed `--client-uuid` of your own. If anything goes wrong, run the same command again: it cannot record the transaction twice.
5. Exports, backups and copies of documents wait for the person to allow them in the app. Tell the user to look at the app, and do not retry in a loop: after a No the token is refused for 30 seconds. Exit code 3 with `denied` means no.
6. Never print the token, and do not put it in a command line, a file in a shared folder or a chat.

*A paragraph to paste into the agent's instructions*

```
You can read and update the user's budget with the `meowny` command (the meowNY desktop app must be running and unlocked).
The token is in $MEOWNY_TOKEN. Run `meowny schema` once to learn the commands and flags.
Always add --json. Data is on stdout; errors are JSON {code, message} on stderr; the exit code is 0 ok,
1 error, 2 bad usage, 3 token refused or app locked, 4 not found. Amounts are strings like "12.50".
Create transactions with your own --client-uuid so a retry cannot duplicate them. Deleting needs --yes: only
do it when the user asked. Exports, backups and copies of documents need the user to click Allow in the app: ask them to, and
do not retry after a denial. Never reveal the token.
```

To read the same facts from a file rather than a terminal, this page is also published as plain Markdown: [CLI.md](CLI.md).

## The HTTP API behind it

The command line is a thin client. Anything it does you can do yourself with `curl`, because the app serves a small HTTP API on this computer only (`127.0.0.1`, a port the app picks and writes to `api.json` in its data folder). It is off until you turn it on.

- Every call is `POST /v1/<route>` with a JSON object as the body (nothing means `{}`) and the header `Authorization: Bearer <token>`.
- The answer is JSON: `{"ok": true, "data": ...}`, or `{"ok": false, "error": {"code": "...", "message": "..."}}` with a matching HTTP status (400, 401, 403, 404, 409, 413, 423, 500).
- The `Host` header must be `127.0.0.1:<port>` or `localhost:<port>`, and a request with an `Origin` header is refused, so a web page in your browser cannot use it.
- Money is text with two decimals (`"12.50"`), ids are text, dates are `YYYY-MM-DD`, months `YYYY-MM`. Unknown or mistyped arguments are a 400 that names them.
- Routes marked "and your yes" wait (up to two minutes) for you to allow them in the app, and answer `denied` if you do not.
- A body is at most 1 MiB, except for `documents.add`: it takes the file as base64url text in the argument `data` (the command line does this for you), so its body may be up to 35 MiB, and is read only after the token and its scope have been checked from the request's head.

*Ask the app for its status*

```
curl -s -X POST "http://127.0.0.1:$PORT/v1/status" \
  -H "Authorization: Bearer $MEOWNY_TOKEN"

{"data":{"api":1,"app":"meowny","schemaVersion":13,"scope":"read","version":"0.1.0"},"ok":true}
```

The port is in `api.json`. `meowny schema --app` prints every route with its arguments as JSON.

| Route | Token | Arguments | What it does |
| --- | --- | --- | --- |
| `status` | read | none | The app version, the API version and the scope of the token used. |
| `schema` | read | none | Every route with its scope and arguments, so a client can discover them. |
| `today` | read | none | Today's date and the current month in the time zone set in the app. |
| `settings.get` | read | none | The currency, number format, time zone and language set in the app. |
| `categories.list` | read | `type` | The categories that are in use, oldest first, with their ids. |
| `transactions.list` | read | `from`, `to`, `categoryId`, `q`, `limit`, `offset` | Transactions, newest first, with the total that match. Up to 100 a page, 50 unless limit says otherwise. |
| `budgets.overview` | read | `month` (required) | For a month, per side (income, expense): the budget, how much of it is given to categories, what is left, and the actual so far. |
| `budgets.history` | read | none | One row per month, newest first, from the first month that had a budget: the budget and the actual for each side. |
| `recurring.list` | read | none | Every recurring item, with its schedule, its next due date and whether it is active. |
| `recurring.dues` | read | `withinDays` | Active recurring items that are overdue (due before today) and those due in the next days. Today is the date in the app's time zone. |
| `stats.budget` | read | `month` (required) | Budget against actual for a month: each category with its budget, what was spent or earned, what is left and the percent used, and the totals. |
| `stats.trend` | read | `categoryId` (required), `from` (required), `to` (required) | One category month by month, with its budget and actual for each month. At most 120 months. |
| `stats.year` | read | `year` (required) | Income, expense and savings for each month of a year. |
| `stats.top` | read | `month` (required), `type` (required), `limit` | The categories with the most spending, or income, in a month, biggest first, with their share of the month. |
| `stats.commitments` | read | none | What the active recurring items cost, or bring in, in an average month. Items with no fixed amount are listed but not counted. |
| `stats.planned` | read | `month` (required) | Planned against paid amounts of the recurring items that were paid in a month. |
| `settings.set` | write | `locale`, `timezone`, `language` | Changes the number and date format, the time zone or the language. Give at least one. The currency is changed in the app. |
| `categories.create` | write | `name` (required), `type` (required) | Makes a category. Its name is 1 to 50 characters and unique among the categories of that type. |
| `categories.rename` | write | `id` (required), `name` (required) | Renames a category. Its type cannot change. |
| `categories.delete` | write | `id` (required) | Deletes a category: it is no longer offered or budgeted, and its transactions keep it. |
| `transactions.create` | write | `name` (required), `amount` (required), `paymentDate` (required), `categoryId` (required), `description`, `clientUuid` | Records a transaction. With a clientUuid, sending the same request again gives back the first transaction instead of making another. |
| `transactions.update` | write | `id` (required), `name`, `amount`, `paymentDate`, `categoryId`, `description` | Changes a transaction. Fields left out stay as they are; a null description clears it. |
| `transactions.delete` | write | `id` (required) | Deletes a transaction. The change history keeps what it said. |
| `budgets.set` | write | `type` (required), `month` (required), `amountLimit` (required), `categoryId` | Sets a monthly budget: the global budget of a side, or one category's. A month that has already started changes only that month; this month and later ones carry the new amount forward. A category budget must match the category's type. |
| `budgets.remove` | write | `type` (required), `month` (required), `categoryId` | Removes a budget, with the same month rules as setting one. The months before keep it, and the history stays. |
| `recurring.create` | write | `name` (required), `amount`, `categoryId` (required), `startDate` (required), `intervalUnit` (required), `intervalCount` (required), `endDate`, `description` | Adds a recurring item: a bill or an income that comes back every so many days, weeks, months or years. |
| `recurring.update` | write | `id` (required), `name`, `amount`, `categoryId`, `startDate`, `intervalUnit`, `intervalCount`, `endDate`, `description` | Changes a recurring item. Fields left out stay as they are; null clears the end date or the description, and a null amount makes it a bill with no fixed amount. |
| `recurring.delete` | write | `id` (required) | Deletes a recurring item. The transactions it already made stay. |
| `recurring.pay` | write | `id` (required), `amount`, `paidOn` | Turns the current occurrence into a transaction and moves the item to the next one. A bill with no fixed amount needs the amount. |
| `recurring.skip` | write | `id` (required) | Passes over the current occurrence without making a transaction. |
| `recurring.pause` | write | `id` (required) | Pauses a recurring item: it stops coming due until it is resumed. |
| `recurring.resume` | write | `id` (required) | Resumes a paused recurring item, unless its next date is past its end date. |
| `recurring.end` | write | `id` (required) | Ends a recurring item for good: it becomes inactive and its end date is today. What it already paid stays. |
| `export.preview` | read | `from`, `to`, `categoryIds`, `includeDebts`, `kind`, `minAmount`, `maxAmount`, `text`, `split`, `sections` | What an export of this selection would contain: how many transactions, their dates, the income and expense totals, and the other records (the friends, the history of debts, the overdue payments, the documents). |
| `backup.schedule` | read | none | The scheduled backup, if one is set up: the folder, how often and how the last one went, or null. Its passphrase is never shown. |
| `export.json` | read, and your yes | `out` (required), `from`, `to`, `categoryIds`, `includeDebts`, `kind`, `minAmount`, `maxAmount`, `text`, `split`, `sections` | Writes the data as a JSON file, after the person allows it in the app: the transactions, categories, budgets, recurring items, documents, and the debts (friends, the overdue recurring payments, the history of debts), or the parts and the limits chosen. An existing file is never replaced. |
| `export.csv` | read, and your yes | `out` (required), `from`, `to`, `categoryIds`, `kind`, `minAmount`, `maxAmount`, `text`, `split` | Writes the transactions as a CSV file, after the person allows it in the app, limited by the dates, the categories, the kind, what they cost you, a text and whether they are shared. An existing file is never replaced. |
| `export.xlsx` | read, and your yes | `out` (required), `from`, `to`, `categoryIds`, `includeDebts`, `kind`, `minAmount`, `maxAmount`, `text`, `split`, `sections` | Writes a workbook (.xlsx) with sheets for transactions, categories, budgets, recurring items, documents, the debts (friends, due payments, history of debts) and a monthly summary, after the person allows it in the app. Dates are ISO and amounts plain numbers. An existing file is never replaced. |
| `export.pdf` | read, and your yes | `out` (required), `fromMonth` (required), `toMonth` (required), `categoryIds`, `includeTransactions`, `showDescriptions`, `page`, `includeDebts`, `includeDuePayments`, `includeDebtHistory` | Writes a PDF report for whole months, in English, after the person allows it in the app: totals, charts and budget against actual, and the transactions if asked. An existing file is never replaced. |
| `backup.create` | read, and your yes | `out` (required), `passphrase` (required) | Writes an encrypted backup of all the data, after the person allows it in the app. It opens only with the passphrase given here, and only the app can restore it. An existing file is never replaced. |
| `documents.list` | read | `q`, `kind`, `from`, `to`, `transactionId`, `recurringId`, `ledgerEntryId`, `limit`, `offset` | Documents, newest paper first, with the total that match: names, dates, types, sizes and hashes, never the files. Up to 100 a page, 50 unless limit says otherwise. |
| `documents.show` | read | `id` (required) | One document: its name, date, type, size and hash, whether this computer holds its file, and in `links` what it is attached to (a debt entry also gives `personId`, the friend). Never the file. |
| `documents.add` | write | `data` (required), `name` (required), `date`, `keepOnPhone`, `transactionIds`, `recurringIds`, `ledgerEntryIds` | Stores a file as a document, encrypted, and attaches it where asked. The answer is the document and `alreadyThere`: the same file added again is not stored twice, and the attachments are added to the document that is there. At most 25 MiB. |
| `documents.update` | write | `id` (required), `name`, `date`, `keepOnPhone` | Renames a document, changes its date, or switches whether the phone keeps its file. Give at least one. The contents cannot be changed. |
| `documents.delete` | write | `id` (required) | Deletes a document, its file and its attachments. The transactions it was attached to stay. |
| `documents.link` | write | `id` (required), `transactionId`, `recurringId`, `ledgerEntryId` | Attaches a document to a transaction, a recurring item or a ledger entry. Giving one that is attached already changes nothing. |
| `documents.unlink` | write | `id` (required), `transactionId`, `recurringId`, `ledgerEntryId` | Detaches a document from a transaction, a recurring item or a ledger entry. The document and the transaction stay. |
| `documents.get` | write, and your yes | `id` (required), `out` (required) | Writes a copy of a document's file, after the person allows it in the app. It is not encrypted. An existing file is never replaced. |
| `people.list` | read | `archived` | The friends with what each owes you (negative: you owe them), the largest first. Archived friends only if asked. |
| `people.show` | read | `id` (required) | One friend and their ledger: their parts of shared bills and the entries by date, with the balance after each line. |
| `debts.summary` | read | none | What friends owe you and what you owe them, in all and for each friend whose balance is not zero. Neither is income or expense. |
| `debts.due` | read | none | The recurring payments that are overdue and not paid or skipped, one for each occurrence that was missed (at most 24 for an item), the oldest first, with the day each fell due, what it is for (null for a bill with no fixed amount), the days late and what they come to. A payment is overdue from the day after its day; income and paused or ended items are not listed. |
| `debts.history` | read | `from`, `to`, `personId`, `limit`, `offset` | The history of debts, newest first, a page at a time: the lines of every friend (lent, borrowed, paid back, their parts of shared bills, with the balance after each) and the recurring payments that were paid late or are overdue. A payment made on its day or before it is not in it. |
| `debts.reminders` | read | `everyDays` (required) | Which friends a reminder is due for today, and the next days with one. Every friend whose balance is not zero is reminded this many days after the day their debt last changed (the latest line of their statement), and again after as many; 0 is none. The app rings them (a device sets its own period and time of day); this only reads what they would be about. |
| `splits.preview` | read | `amount` (required), `mode` (required), `mine`, `people`, `guests`, `values` | What a split would give each person for a bill of this amount, without saving anything: your part, theirs, whether the numbers add up and how much is left. |
| `people.create` | write | `name` (required) | Adds a friend. A friend is a name, 1 to 50 characters, unique among the friends who are not archived. Nothing else about a person is kept. |
| `people.update` | write | `id` (required), `name`, `remindOn`, `remindEveryDays` | Renames a friend. Fields left out stay as they are. The reminder fields are kept for data written before reminders became a setting of each device (every so many days after a debt last changed), and nothing rings from them. |
| `people.archive` | write | `id` (required) | Hides a friend from the lists and pickers. Their balance and history stay, and the name can be used again. |
| `people.unarchive` | write | `id` (required) | Brings back an archived friend, if no other friend has taken the name. |
| `people.settle` | write | `id` (required), `amount`, `date`, `note` | Marks a payment between you and a friend: they paid you back if they owe you, you paid them back if you owe them. With no amount it is the whole balance. An amount larger than the debt is accepted: the balance turns around and the answer says how much was over. |
| `ledger.add` | write | `personId` (required), `kind` (required), `amount` (required), `date` (required), `note`, `transactionId`, `expenseName`, `expenseCategoryId` | Records money lent, borrowed or paid back with a friend. It is never income or expense. For a borrowing, give expenseName and expenseCategoryId to add your share to your expenses in the same step, with the entry linked to it. |
| `ledger.update` | write | `id` (required), `personId`, `kind`, `amount`, `date`, `note`, `transactionId` | Changes an entry of the ledger. Fields left out stay as they are; a null note or transactionId clears it. The expense it came with is not changed. |
| `ledger.delete` | write | `id` (required) | Deletes an entry of the ledger. The balance of the friend changes by that much. The change history keeps what it said. |
| `transactions.split` | write | `id` (required), `mode` (required), `mine`, `people`, `guests`, `values` | Shares a transaction with friends and guests. It keeps the amount that was paid; your part, the amount minus theirs, is what budgets and statistics count. The numbers must add up. Only an expense can be shared. |
| `transactions.unsplit` | write | `id` (required) | Takes the split off a transaction: it costs you its whole amount again, and the friends' balances go down by their parts. |
| `recurring.split` | write | `id` (required), `mode` (required), `mine`, `people`, `guests`, `values` | Splits every payment of a recurring item the same way, from now on. Kept as proportions, so a payment that differs from the plan is shared in the same proportions. EXACT needs an item with an amount that the values add up to. Payments already made are not changed. |
| `recurring.unsplit` | write | `id` (required) | Takes the split off a recurring item: its payments are not split from now on. Payments already made keep theirs. |
| `export.debts` | read, and your yes | `out` (required), `what`, `format`, `personId` | Writes who owes what to a text, CSV or PDF file, in English, after the person allows it in the app: the SUMMARY of all friends, the STATEMENT of one friend (personId), or the LEDGER of every entry as CSV. An existing file is never replaced. |

## Security

- **Off by default.** Nothing listens until you turn command line access on in the app.
- **This computer only.** The app listens on `127.0.0.1`, never on the network, on a port the system picks.
- **A token for every request,** checked against fingerprints kept in your encrypted data. A locked app cannot check any token, so it answers "locked" and gives nothing.
- **Web pages cannot use it.** A request that carries an `Origin` header, or the wrong `Host`, is refused, and the API sends no cross-origin permissions.
- **Least power by default.** Tokens are read only unless you choose otherwise, expire, and can be revoked.
- **Your yes for files,** each time, with the exact file shown, and never over an existing file.
- **No restore, no wipe, no currency change** through it, whatever the token.
- **It does not keep the app awake.** Calls do not count as activity, so the idle lock still works.
- **Nothing leaves the computer.** The command line only ever connects to the app on this computer. There is no telemetry.

> **Careful:** A leaked read-only token lets someone who can reach this computer read all your data until it expires; a leaked read-and-write token lets them change it too. Revoke a token in Settings, command line access, as soon as you suspect it. The saved token file is protected only by file permissions.

To report a problem, open an issue at [the project page](https://github.com/v1nc3t/meowNY).

meowNY is free software (AGPL-3.0). Source: [github.com/v1nc3t/meowNY](https://github.com/v1nc3t/meowNY). This page was generated from `meowny` 0.1.0 (API 1); its examples are real output.
