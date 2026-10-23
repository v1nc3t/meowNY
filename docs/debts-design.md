# Debts and expense splitting design (phase 10)

Status: built (batches 1 to 3, sections 19 to 21). Decisions come from phase 10 of [PLAN.md](PLAN.md). Section 16 lists open points.

## 1. What it has to do

- Keep **friends**: a name and nothing else (plus an optional reminder, section 11). Nothing is imported from contacts.
- **Split a bill**: an expense you paid is shared with people equally, by percent, by shares or by exact amounts. The transaction stays what you paid; **your expenses, budgets and statistics show only your part**.
- **Split a recurring item** too (rent with a flatmate): the item carries a template and each payment is split by itself.
- Show **who owes whom**: a balance per friend (positive: they owe you) from split parts and ledger entries (a loan, a borrowing, a repayment).
- **Mark as paid** with an amount. An amount above the debt is accepted and turns the balance around.
- **Share a bill as a QR code.** A friend with the app scans it, sees the details, accepts or denies. Accepting makes a debt in their ledger and, if ticked, an expense of their part.
- **Remind** you of a debt (section 11).
- It follows the rest of the app: sync, currency change, backup, exports, CLI, "Delete all my data".
- Nothing opens a network connection. A QR code carries the bill itself.

Not goals: chat or messaging (needs a server someone runs), importing contacts, anything about a friend but the name, accounts for friends, any server, automatic messages, knowing whether a friend paid, more than one currency per ledger, splitting income, interest or schedules, a pending state on the receiver's device.

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Person | A friend: name, archived or not, optional reminder | `people` |
| Split line | One friend's (or guest's) part of a bill | `transaction_split_lines`, a child of a transaction |
| Ledger entry | A loan, borrowing or repayment with one friend | `ledger_entries` |
| Split template | The proportions of a recurring item's split | `recurring_split_lines` and `recurring_transactions.split_my_weight` |
| Cost view | What a transaction costs you: amount minus the lines | SQL view `transaction_costs` |
| Balance view | What a friend owes you, or you owe them | SQL view `person_balances` |
| Bill code | A QR code (or text) with one friend's part of one bill | Made on screen, stored nowhere |

Your own part is **never stored**: it is `amount - sum of the lines`. So rounding or sync cannot make parts disagree with the total.

## 3. The numbers

Amounts are integers in minor units.

- **Cost of a transaction** = `amount_minor - sum(its lines)`. Unsplit: its amount.
- **Balance of a person** (positive: they owe you) = sum of their split lines + signed ledger entries:

| Entry kind | Meaning | Effect on the balance |
|---|---|---|
| `LENT` | I lent them money | + amount |
| `BORROWED` | I borrowed from them | - amount |
| `REPAID_TO_ME` | They paid me back | - amount |
| `REPAID_BY_ME` | I paid them back | + amount |

- A split line counts like a `LENT` entry dated the day of the bill.
- The balance is a running total: a repayment is not matched to a bill.
- **Owed to you** is the sum of positive balances, **you owe** the sum of negative ones. Neither is income or expense, and neither touches a budget.

## 4. The rows (migration `0012_debts.sql`, schema 12)

```
people(id, sync_id, stamp, name, archived, merged_into, remind_on, remind_every_days, created_at, updated_at)
  name: 1..50, the same name rule as categories; unique among active people, ignoring case
  remind_on: a date or NULL; remind_every_days: 1..365 or NULL
  A person is never deleted, only archived (hidden from pickers, balance and history kept).

ledger_entries(id, sync_id, stamp, person_id, kind, amount_minor, entry_date, note, transaction_id, origin_id, created_at, updated_at)
  kind: LENT | BORROWED | REPAID_TO_ME | REPAID_BY_ME;  amount_minor: 1..999999999999
  note: at most 200 characters, no control characters;  entry_date: a real date
  transaction_id: NULL or a transaction, ON DELETE SET NULL (the debt outlives the expense)
  origin_id: NULL, or the 32 hex characters of a bill code this entry came from; unique

transaction_split_lines(id, transaction_id, person_id, guest_name, amount_minor)
  transaction_id: ON DELETE CASCADE
  exactly one of person_id and guest_name; guest_name 1..50
  amount_minor: 1..999999999999; one line per person per transaction; at most 30 lines
  No sync_id, stamp or marker of its own: the lines belong to the transaction (section 9).

recurring_split_lines(id, recurring_transaction_id, person_id, guest_name, weight)
  recurring_transaction_id: ON DELETE CASCADE; exactly one of person_id and guest_name; guest_name 1..50
  weight: 1..999999999999; one line per person; at most 30 lines. No sync_id, stamp or marker of its own.
recurring_transactions.split_my_weight: NULL (not split) or 0..999999999999 (your weight). Lines exist only when it is not NULL,
  and a template has at least one weight above 0 in all.

document_links gets a third target, ledger_entry_id (the table is rebuilt, as section 5 of documents-design.md said it would be).
```

- **A guest** is a name typed on a bill for someone you do not track: their part leaves your cost and makes no debt.
- Triggers: stamps, deletion markers (ledger entries only), `updated_at` and change history, like other tables.
- **A line is only for an expense**: a trigger refuses a line on an `INCOME` transaction or a template line on an income item or one with no `split_my_weight`.
- **Parts never exceed the bill**: a trigger refuses a line or amount change that makes the lines sum above the amount. These checks stand aside while a sync, restore or currency conversion writes; each checks the finished state afterwards (`splits::check`).
- **Changing a line changes its parent's stamp**: a trigger on the lines runs a no-op `UPDATE transactions SET name = name` (same for template lines and their item), so a split edited on one device is a newer transaction on the next sync.
- **Views**: `transaction_costs(transaction_id, cost_minor)` and `person_balances(person_id, balance_minor)`, with indexes on `transaction_split_lines(transaction_id)` and `(person_id)`, and `ledger_entries(person_id)`.

## 5. Splitting a bill

**The dialog.** On an expense, "Split this bill" opens participants: you (first) and friends or guests, starting equal.

| Mode | You type | Rule |
|---|---|---|
| Equal | nothing | Everyone has weight 1 |
| Percent | each person's percent, two decimals | Exactly 100.00 |
| Shares | a whole number each (2, 1, 1) | At least one above 0 |
| Exact | each person's amount | Exactly the bill |

- A footer shows "total, left to assign"; Save is off until nothing is left. "Give the rest to me" fills your row. A weight of 0 is allowed; a friend with 0 is dropped on save.
- **One function.** `splits::allocate(total, weights)` turns weights into amounts that add up to the total: quota `total x w / W` in 128-bit integers, floor, then leftover cents one each to the largest remainders `(total x w) mod W`, ties to the lower position (you are 0). The dialog asks Rust for a preview on every change (`split_preview`); **the interface has no copy of the rule**.
- **What is stored:** only the amounts of the friends' and guests' lines, not the mode. A saved bill opens as Exact. This loses a typed "40 / 60" and spares a mode and weights that would have to agree through every sync and conversion.
- **Editing.** A new amount rescales every part including yours: `allocate(new total, [old parts])`. Clearing the split deletes the lines. Date or category changes touch no line. **Deleting** a split transaction removes its lines and so changes balances; a repayment already marked stays, so a balance can go negative.
- **Recurring items.** The template is kept as **weights**, not amounts (a payment can differ from plan, an item can have no amount): your weight in `split_my_weight` and one per friend or guest. Equal gives 1 each, shares are the numbers typed, percent is in hundredths (10,000 in all), exact amounts are kept as weights too. When the item is paid (`recurring::pay`), `allocate(amount, weights)` makes the lines in the same database transaction (a part of 0 is dropped); then it is an ordinary split transaction. **Editing the template changes payments to come, never ones already made.** An archived friend keeps their place. Income items cannot have a template.

## 6. The ledger

- **Entries by hand**: I lent, I borrowed, they paid me back, I paid them back. Each has a friend, date, amount, note, optional link to a transaction and optional documents.
- **Mark as paid** opens a dialog starting at the whole balance, adding `REPAID_TO_ME` or `REPAID_BY_ME`. An amount **larger than the balance** is accepted; the dialog says first "this is 15.00 more than Anna owes you; the rest will count as you owing Anna". At zero balance the friend's reminder is cleared.
- **I borrowed, with an expense.** A box "this was my share of a bill: add it to my expenses" (with a category) makes the expense and the `BORROWED` entry linked to it in one database transaction. Paying back later is a `REPAID_BY_ME` entry, never a second expense. A scanned bill (section 10) ends in the same function.
- **Entries never touch transactions.** Editing or deleting the linked expense does not change the entry (delete only clears the link).
- **Never income, never in a budget.** No query that sums income or expense reads `ledger_entries`.

## 7. Every sum uses the cost

The rule: **no query adds up `transactions.amount_minor` for a figure about what you spent or earned; it adds `transaction_costs.cost_minor`.**

| Where | What | Uses the cost |
|---|---|---|
| `budgets.rs`: `actual_minor`, monthly history | Spent and earned against a budget | yes |
| `stats.rs`: category actuals, monthly totals, largest categories | Dashboard, trends | yes |
| `stats.rs`: `commitments` | Active recurring items in an average month | your part of the planned amount: `allocate(amount, weights)`, position 0 |
| `stats.rs`: planned against paid | The bill against its plan | **no**: both sides are the whole bill (about how the price moved). On the guard's list with that reason |
| `report.rs`: category totals | PDF report | yes |
| `export.rs`: monthly summary and totals sheet | JSON, CSV, Excel summaries | yes |
| `export.rs`: list of transactions | Rows | both: `amount` (paid) and `my_share` |
| API and CLI | Routes returning a transaction or total | `my_share` next to `amount`; totals use the cost |
| Screens | Transaction list shows your part as its amount | the cost |

- **The guard.** A test reads the SQL strings in `src-tauri/src` and fails when one sums `amount_minor` of `transactions` and is not on a short list with a reason each (the view itself; planned against paid; recurring planned amounts; backup, sync and currency code). **Random tests** check every figure of every module against a recomputation from the parts.
- **Speed.** Benchmark (`cargo run --release --example bench`, 60,000 transactions, a split on one in five): every list, search, budget and statistics query must stay under 40 ms; else the sums join a pre-summed table of lines.

## 8. Currency change

The conversion that converts transactions, recurring items and budgets (`currency.rs`, one rate, half up) also converts the new rows, in `change` and `apply_recorded`, so both devices agree:

- **Ledger entries:** each amount converted, at least one cent.
- **A split bill:** the amount is converted, then the parts, yours included, are **re-allocated** to it with the old parts as weights. A part that comes out 0 is dropped. Plain rounding of each line could make lines exceed the bill.
- **A split template** holds proportions and is left alone.
- **Balances** are sums of converted pieces, so may differ from the converted old balance by a few cents; every device has the same number. No stamp changes.

## 9. Sync

Same rules as `docs/sync-design.md`. Protocol 3 became **4**, schema 12.

- **`people` and `ledger_entries`** are ordinary tables (`sync_id`, `stamp`, hashes, digests, review dialog; ledger entries leave a deletion marker).
- **A bill's lines travel inside its transaction's row** as `[{p: person sync_id or null, g: guest name or null, a: amount}]`. The winning transaction replaces all local lines, so two devices never end with parts from both. Lines have no marker; a deleted transaction takes them.
- **A recurring item's template travels inside its row**: `split: {mine: w, lines: [{p, g, w}]}`, or none.
- **Apply order:** shared settings, categories, **people**, recurring items, budgets, transactions (with lines), **ledger entries**, documents, document links, then markers.

| Case | Rule |
|---|---|
| Person edited on both | Newer whole row wins; a real conflict goes to the review dialog |
| Same-name active person on both | Lower `sync_id` kept; the other is marked deleted with `merged_into`; transactions, recurring items and entries move to the kept one with `moved` stamps. Two lines for one person become one (amounts added; template: weights added) |
| A rename onto a name another person is giving up, a swap of names | Same waiting and temporary names as categories |
| Entry added on both | Both kept. Same `origin_id` is the same scanned bill: newer stays, other becomes a marker |
| Entry edited on both, or edited on one and deleted on the other | Newer wins |
| Split edited on both, or transaction on one and split on the other | Newer whole transaction wins, with its lines |
| Recurring item or template edited on both | Newer whole row wins, with its template. A payment made on both keeps the existing rule |
| Transaction deleted on one, entry linked on the other | Entry stays, link cleared (superseded by migration `0013`: it takes the deletion's stamp) |
| Document linked to a ledger entry | Section 10 of the documents design; a link for a deleted entry is not taken and not an error |
| Person archived on one while the other adds a line or entry | Both kept |

Row checks on arrival: name rule, closed kinds, amounts in range, real dates, note length, `origin_id` of 32 hex, an unknown person or transaction that is not a known deletion stops the sync. Transactions: lines at most 30, each exactly one of person and guest, no person twice, **lines add up to no more than the amount**, expense category. Recurring items: template of at most 30 lines, weights in range, one above 0, expense category. A failure ends the sync naming the table, never the content. The currency generations cover the new rows.

## 10. Bill codes (QR)

For each friend (not a guest) on a split bill, "show code" shows a QR code of that friend's part; the same text can be copied. The code is a notice, not signed or encrypted; the friend decides.

**The text.** `MNB1:` plus base64url of a JSON object (a different prefix from the pairing code's `MNY1:`):

```
{"v":1, "id":"<32 hex>", "from":"<name, 1..50>", "what":"<1..60>", "date":"YYYY-MM-DD", "amt":2500, "total":10000, "cur":"EUR"}
```

- `amt` is the friend's part, `total` the whole bill. No other person's name or part.
- `id` is the first 16 bytes of `SHA-256("meowny-bill-1" || transaction sync_id || person sync_id)` in hex: stable per bill and friend, reveals no sender id.
- `from` is the sender's chosen name, remembered on the device, not synced.
- At most 1 KiB. A snapshot: later edits do not follow, and the friend's side tells the sender nothing.

**Scanning.** On the phone the sync scanner (`ui/src/api/scanner.ts`) opens from "scan a bill"; the desktop has "paste a code". Both call `bill_preview(text)`, which parses strictly (size, prefix, version, exact fields, lengths, no control or direction characters, `amt` and `total` in range, `amt <= total`, real date, three-letter currency). **A scan writes nothing.**

**The dialog** shows plain text (never markup): sender, what, date, "your part 25.00 EUR of 100.00", and:

- **Which friend:** the one matching `from` (ignoring case) preselected, or "new friend" with the name filled in.
- **Add to my expenses:** ticked by default, with a category (last used for this friend suggested).
- **Currency:** when `cur` differs, ask for the rate ("1 EUR = ___ USD") and show the converted amount, editable. Accept stays off until there is one. Nothing is fetched.
- **Accept** and **Deny** (Deny stores nothing). A code already accepted (an entry with this `origin_id`) is not accepted again.

**Accept** is one database transaction (`bill_accept`): the friend if new, the expense when ticked, and the `BORROWED` entry (converted part, bill date, a note, the `origin_id`) linked to the expense. The same function serves "I borrowed, with an expense".

The sender never learns the code was scanned or that the friend paid. A friend wanting a correction asks for a new code: delete the old entry (which frees its `origin_id`) and accept the new one. A code reaching the wrong person tells them a name, a bill and a sum, which is why it shows only while the dialog is open and the phone window is secure.

## 11. Reminders

**Changed 2026-10-17:** the reminder is one setting of each device (never, every week, every 2 weeks, every month; default every week; webview storage `debt-reminder-every`) applying to **every friend whose balance is not 0**: the first reminder comes that many days after the day of the latest line of their statement, then every period until settled; a new line restarts the count. `reminders::reminders(conn, today, every_days)` works it out from the statements, so nothing new is stored. The friend's `remind_on` and `remind_every_days` stay in the data, sync, backups and API (to lose nothing and avoid a migration) and **nothing reads them**; the friend's page has no reminder block. The rest of this section still holds.

(Before that change:) a reminder belonged to a friend: `remind_on` and optionally `remind_every_days`, live while the balance is not 0. Nothing is written when a reminder fires; each device keeps in webview storage which date it has shown. The time of day is a per-device setting (09:00 default).

The text is **"meowNY: a debt reminder"** with a count, never a name or amount: the app is locked when it shows, and a lock screen can be seen and notification history kept. The debts page then names who is due.

- **Desktop.** The recurring-items notification (`useDueNotifications`) is extended: while the app is open and unlocked, at the chosen time or after, one notification for due reminders not yet shown. One that came due while closed is listed on the debts page and in one notification at the next unlock. The "notifications" switch turns these off too.
- **Phone.** For every friend with a live reminder the app schedules the next four occurrences with the system (so they fire with the app closed), asks for the notification permission the first time, and schedules again at each unlock, after each sync and when a reminder changes. The text is fixed at scheduling. A friend settled by sync on an unopened phone keeps their notifications until the next unlock (accepted, said in settings). **Not known until a real phone has run it:** whether the notification plugin can schedule on Android, the permission and exact-alarm rules of Android 13+, and behaviour with the window secure (section 21 checklist).

## 12. Backup, restore, exports, "Delete all my data"

- **Backup and restore:** `people`, `ledger_entries`, `transaction_split_lines`, `recurring_split_lines` join `TABLES` in `backup.rs`. Restore validates by section 9's rules and refuses the file whole when one fails; a restore gives a new device id and unpairs.
- **"Delete all my data"** removes the database, which holds all of it. Bill codes are not stored.
- **Exports.** JSON, CSV and Excel gain people, ledger entries and every bill's lines; transaction rows have `amount` and `my_share`. A limited export includes the people its transactions mention and no ledger entries unless "debts" is ticked. The summary of who owes what, or one friend's statement, is saved as **text, CSV or PDF** for you to send yourself.
- The monthly PDF and summaries use the cost (section 7).

## 13. Command line and API

New routes named like the existing ones, with commands by construction through `cli/schema.json`. Read: people, a person's ledger, balances. Write: add, rename, archive and set the reminder of a person; add, edit, delete a ledger entry; mark as paid (`settle`); set and clear a transaction's split (`split_set` takes mode and values, the app allocates); preview a split; recurring routes take and return a template. Transaction routes return `my_share` and the split beside `amount`. The scan and acceptance of a bill code, showing a code and notifications are not in the API: they need the person at the app. Write routes need a write token.

## 14. The screens

Desktop and phone, seven languages (the i18n key test covers them).

- **Debts page** (sidebar; phone top-bar icon). Changed 2026-10-17: tabs friends, due payments and history of debts; no totals or saving on the page. Each friend has *edit*, *delete* (takes them off the list, as "remove" always was; balance and history stay; "show removed friends" and *bring back* undo it) and *mark as paid*. Buttons "lend", "borrow", "add friend", "scan a bill" (phone), "paste a code" (desktop). A line above the list names due reminders.
- **A friend's page:** name, balance, ledger by date with balance after each, documents, "statement".
- **The bill dialog** (in the transaction form), section 5; each friend line has "show code".
- **The recurring form** has the same split switch and dialog; the list marks split items and templates naming an archived friend.
- **Transaction list:** your part is the amount; details show what was paid and who has which part.
- **Dashboard:** a small card with the two numbers, opening the debts page.
- **Settings:** the reminder time and period of this device, and what the desktop and phone can do.
- The sync review dialog and summary know the new rows.

## 15. Threats and what stops them

| Threat | What stops it |
|---|---|
| A code or pasted text built to break the parser, or very large | One strict parser with a size limit, fixed fields and version; no allocation from a length in the text; a fuzz test; nothing written before Accept |
| A code that says "you owe me 500" | The friend sees details and decides; no inbox, no default; Deny stores nothing |
| Markup or tricks in names | Shown as text only; control and direction characters refused |
| The same code accepted twice | `origin_id` is unique; the dialog says it was added |
| A code read from your screen | Shown only while a dialog is open; secure phone window; holds no other person's data |
| A notification shows a debt on a lock screen | Fixed text, no name or amount |
| Sums that forget the split | One view, the section 7 guard test, random tests |
| Parts that disagree with the bill | Your part is derived; one `allocate`; lines travel with their transaction; checks on every outside row |
| A damaged or hostile sync or backup row | Section 9 checks end the sync or refuse the file, naming the table |
| A split deleted by mistake | The confirmation names the thing |

## 16. Open points

- A bill code for a loan (not tied to an expense): the text has a version and room for a `kind`; not built.
- A code back ("I paid you"): not built.
- The phone's reminders (section 11) wait for a real phone.
- The desktop paste box is a proposal; droppable if "scan only" is preferred.
- Several currencies per friend: not supported.
- Amount filters of the transaction list keep using the paid amount; the list shows both.
- A review by someone else of the bill-code parser before release.

## 17. Tests

- **Allocation:** property tests (parts add up, each within a cent of its quota, depends only on weights and positions, zero weight gets zero); fixed cases (100.00 among three; 0.01 among two; one huge weight); percent not adding to 100.00 and exact amounts not adding up are refused.
- **Cost and balance:** the views against a plain recomputation on random bills, entries, edits and deletes; a repayment above the debt turns the balance; archiving changes none.
- **Every sum uses the cost:** the section 7 guard, and random bills through every module against a recomputation.
- **Never income:** nothing in the ledger appears in income, a budget or a statistic.
- **Rows:** constraints and triggers (kinds, ranges, one line per person, lines never above the bill, no line on income, stamps, markers, audit).
- **Templates:** paying makes lines adding up to the payment (planned amount exactly as typed, other amounts, no-amount items); editing a template leaves made payments alone; no template on income; the average-month figure uses your part; the same occurrence paid on two devices ends with one transaction; weights survive a currency change.
- **Currency:** lines keep adding up, only a part that becomes 0 is dropped, entries converted, same result on both devices.
- **Sync:** every section 9 case; random edits converge; a split edited on both ends with one side's parts; same-name friends merge; hostile rows stop the sync.
- **Bill codes:** round trips; every malformed input refused and a fuzz run that never panics; a scan writes nothing; Accept writes friend, expense and entry together or nothing; the same code twice; another currency with a rate; no other person's name or part.
- **Reminders:** which dates are due, none for balance 0, text never holds a name or amount.
- **Backup:** round trip of all tables; a hostile file refused whole; "Delete all my data" leaves no table.
- **Screens:** looked at in a browser at desktop and phone width with commands faked.
- **By hand:** the plan's "done when" on a desktop and real phone, with a second phone for the code.

## 18. Batches

1. **The numbers.** This design, migration `0012`, `splits.rs`, people, ledger, mark as paid, the borrowing with an expense, the split template and its payments, the views and the cost in every sum with the guard, currency change, backup and restore, exports and summary, API and CLI. No screens.
2. **The screens and the sync.** Debts page, friend's page, bill dialog (and the recurring form), list marks, dashboard card, documents on entries, seven languages, protocol 4 with merge rules and convergence tests, the phone.
3. **Bill codes and reminders.** The code, scan and paste, the dialog with conversion, accept and deny, reminders on desktop and phone, the guide, the data inventory and the real-phone checks.

## 19. Built in batch 1 (2026-10-17)

The numbers, with no screens: migration `0012_debts.sql`, `src-tauri/src/debts/` (`splits.rs`, `people.rs`, `ledger.rs`, `summary.rs`), the cost in every sum, currency change, backup, exports, API and CLI. What differs from sections 1 to 18:

- **Views.** `transaction_costs` also carries `category_id` and `payment_date`, so queries read the view in place of the table. The subquery runs through `idx_split_lines_transaction` `(transaction_id, amount_minor)`. The guard test is `debts/cost_tests.rs`; two queries are listed with a reason.
- **Speed.** With one expense in five shared, every list, search and statistics query is under 20 ms. **`budgets::history` (ten years in one pass) takes 50 ms (36 ms before)**, over the 40 ms line. A pre-summed table was 17% faster in a trial and 20 times slower for one month, so it was not used; a signed list summed without a subquery is another way. Left for the maintainer.
- **Checks that stand aside** while `sync_clock.applying` is on; `splits::check` runs at the end of restore (refused whole with `InvalidBackup`), conversion (rolled back) and sync (error). Parts cannot change bill or person.
- **A ledger entry's link** is cleared by the foreign key without a stamp (superseded by `0013`).
- **People.** Names are unique among non-archived people, ignoring case (ASCII only, as SQLite). An archived friend with a balance stays in the totals. A repayment can be recorded for an archived friend. A guest name is unique on a bill, ignoring case.
- **Editing a split.** `set_split` touches only differing parts, in an order that never lets parts pass the bill. In percent and exact mode a missing `mine` is the rest. The preview returns `complete`, `left` and amounts and never writes.
- **Exports.** JSON has `people`, `ledger` and per transaction `myShare` and `split`; CSV a last column `my_share`; Excel `My share` plus sheets `People`, `Ledger`, `Split bills`; `includeDebts` brings the ledger into a limited export, which otherwise names only the friends on its bills, with no balance or reminder.
- **Summaries.** `summary.rs` writes the summary and a friend's statement as text or CSV, and the ledger as CSV, in English (`summary::Labels` for the screens).
- **API and CLI.** Seventeen routes: read `people.list`, `people.show`, `debts.summary`, `splits.preview`; write `people.create`, `people.update`, `people.archive`, `people.unarchive` (not "restore": a test forbids that word in route names), `people.settle`, `ledger.add`, `ledger.update`, `ledger.delete`, `transactions.split`, `transactions.unsplit`, `recurring.split`, `recurring.unsplit`; and `export.debts` (needs the person's yes). A split is typed with flags `mode`, `mine`, `people`, `guests`, `values`.

## 20. Built in batch 2 (2026-10-17)

The screens and the sync. What differs from sections 1 to 19:

- **Tauri commands:** seventeen (`list_people`, `create_person`, `update_person`, `archive_person`, `unarchive_person`, `person_statement`, `debts_summary`, `add_ledger_entry`, `update_ledger_entry`, `delete_ledger_entry`, `settle_person`, `split_preview`, `set_transaction_split`, `clear_transaction_split`, `set_recurring_split`, `clear_recurring_split`, `export_debts`). A test fails when one is missing from `COMMANDS`, `build.rs`, `capabilities/default.json` or the handler; `debts/command_tests.rs` calls them through Tauri's mock runtime.
- **The PDF** (`report/debts.rs`, krilla; desktop only): words come from the screen's language via `ui/src/api/debtsLabels.ts` (a test compares its fields with `summary::Labels`); the app refuses a word over 120 characters or with a control character; CLI and API use English.
- **Documents on entries:** library, `documents.add|link|unlink|list` (`ledgerEntryId`), counts, export and sync.
- **Sync, protocol 4:** parts inside the transaction's row (`split: [{person, guest, amountMinor}]`), template inside the item's (`split: {mine, parts}`); the review dialog names the tables and fields; `splits::keep_valid_after_overwrite` is gone, the end-of-`apply` check `splits::check` stays. Migration `0013` (schema 13): entries of a deleted expense take the deletion's stamp.
- **A friend merged away** is hidden from every read (`merged_into IS NOT NULL`): not listed, found, offered, or brought back by `unarchive`.
- **Odd cent after conversion** goes by position; `rescale` sorts friends by `sync_id`, then guests by name.
- **Smaller than the design:** a saved bill opens as exact amounts and a saved template as shares (the design's "start from what was saved" would need arithmetic in the interface). A friend's reminder was shown, not set from the screens (batch 3). The export page has no "include the ledger" choice yet.
- **Open:** `budgets::history` (50 ms against 40 ms).

## 21. Built in batch 3 (2026-10-17)

The bill codes and the reminders, and the guide. What differs from sections 1 to 20, found while building:

- **The code** (`src-tauri/src/debts/codes.rs`). The text is `MNB1:` and the base64url of the JSON of section 10, field for field, and `parse` is the only way in: at most 1,024 bytes, the exact prefix, the strict alphabet (no padding), no field missing or unknown, version 1, an id of 32 lowercase hex characters, `from` 1 to 50 and `what` 1 to 60 characters with no space at either end, a real date, `amt` from 1 to the largest amount and `total` from `amt` to it, a currency of three capital letters. Names refuse control characters and also the invisible ones that change how text is laid out (direction marks, overrides and isolates, zero-width characters, the byte order mark). `make` refuses a friend who is not a part of the bill (a guest has no code) and checks that what it made is what `parse` takes. 10 tests, one a fuzz run of 30,000 damaged codes (it never panics, and what it accepts is a code the app could have written).
- **The commands** `bill_code`, `bill_preview`, `bill_accept` and `debts_reminders` (listed, permitted and in the handler, with mock-runtime tests). None is in the API or the command line: they need the person at the app. `bill_preview` writes nothing, not even to the history (a test counts the rows before and after). `bill_accept` is one database transaction: the friend (if new), the expense (if ticked) and the `BORROWED` entry with the code's id as its `origin_id`, linked to the expense. The same code is not taken twice (until its entry is deleted); a choice that cannot work (no friend, two friends, an archived friend, an income category, a name that is taken, a rate for a code in the app's own currency) is refused and nothing is left, not even the new friend. For another currency the part is converted with the typed rate (`currency::convert_at`: the same rule and rounding as a change of currency) or the amount the person typed instead.
- **The note of an entry from a code** is the name of the bill (`Dinner`), and `Dinner: 25.00 USD at 0.92` when converted; the design said only the second. A statement then says what each line was.
- **The sender's name** is kept in the webview's storage of this device (`bill-sender-name`), asked on the first-run screen (changed on 2026-10-18; it was asked at the first code, which stays as the way in for a vault with no name, such as a restored one) and changeable in the settings, as `due-notified` is. It is not in the database, so it is not synced, backed up or in an export.
- **A QR code of lowest error correction.** The encoder made for the sync codes goes up to version 10, 271 bytes at the lowest level (213 at the level the pairing code uses). A code of ordinary names is 190 to 230 characters and fits at level L; one with very long names does not, and the dialog says so and leaves the text to copy (the paste box takes it on the other side). The encoder was not extended.
- **Taking a code in on two devices** makes two entries with one code id, which the sync turns into one (the newer stays, as section 9 says), and one friend; but each device made its own expense, and both stay. A test says so. Take a code in on one device.
- **Reminders** (`src-tauri/src/debts/reminders.rs`). `debts_reminders` says which friends are due (the last day of their reminder that is not after today, while their balance is not 0, archived friends included) and the next four distinct days with how many friends have one. The time of day, the day this device has shown and the notification are all the page's (`ui/src/lib/reminders.ts`, tested): `ringNow` picks the friends whose day has come and was not shown on this device yet (at once when the day is past, at the time of day when it is today), `schedulePlan` the days to hand the system, with one id for each day (`900000000` plus the day as a number, so a day replaces itself). **Nothing is written when a reminder comes due.**
- **On the computer** `useDebtReminders` checks every half hour while the app is open and unlocked, and when something changes a reminder or a balance. The text is `meowNY: a debt reminder` and `friends to look at: {count}` (a test reads the text of every language and refuses a name, an amount or a digit). The switch "notifications" of the settings turns it off with the recurring items'.
- **On the phone** `scheduleOnPhone` cancels what this device scheduled (the ids are kept in `debt-scheduled`) and schedules the next days at the time of day, through the notification plugin's own commands (`notify` with a `schedule`, `cancel`, `is_permission_granted`, `request_permission`), called by name as the camera is, so no package and no Rust that only a phone compiles. The capability `mobile.json` grants those four. It runs at each unlock, after each sync, when a reminder, a balance or the time changes, and the permission is asked the first time there is a day to ring on; a "no" is said on the friend's page. The schedule is inexact (`allowWhileIdle` off), because an exact one needs a special permission on Android 12 and later; a reminder may come a little late.
- **The screens.** The friend's page has a reminder (a day, a repeat of 1 to 365 days or none, clear); the debts page has **scan a bill** (phone), **paste a code**, a line naming the friends whose reminder has come and `reminder came {date}` on their row; the share mark's detail on the transaction list and a bill line in a friend's page have **show code**; the settings have the time of day and say plainly what the computer and the phone can do. Texts in all seven languages (44 new, with the two for scanning and long codes).
- **Splitting made easy to find.** The bill dialog was in batch 2 behind a checkbox at the end of the form, which is easy to miss, so: **split a bill** beside **add** on the transactions page, the recurring page, the debts page and a friend's page (these two open a new transaction with the split on, and the friend's page puts that friend on the bill); **split** (**edit split**) on every expense in the lists; and the switch now sits under the date, before the description, in both forms.
- **Not known until a real phone has run it** (the spike of section 16): whether the notification plugin schedules on Android as used here (the `notify` command with a `schedule` and an id), whether Android 13 and later asks for the permission when the first reminder is set and what an inexact alarm does under battery saving, whether notifications ring with the app closed and the window secure, whether a code of 200 characters scans reliably from a screen at level L, and the camera for a bill code (the same plugin as the pairing). The checklist is below.
- **Looked at, not only tested.** The code dialog (the QR code drawn from a real-length code), the dialog that takes a code in (the same currency, and another with a rate and the converted amount), the friend's page with its reminder, the debts page with the line of due reminders, and the **split a bill** dialog, in a browser with the commands faked; the scan and the schedule can only be tried on a phone.

**For a real phone** (the maintainer, with a second phone): (1) a split bill on the computer, **show code** for a friend; sync; on the second phone **scan a bill**, check the dialog (name, bill, date, part), accept, and see the entry and the expense; (2) the same code a second time says it was taken; (3) a code in another currency asks for the rate; (4) on the phone set a reminder for tomorrow a minute from now by changing the time of day; close the app; see the notification, with no name or amount on the lock screen; (5) set one for a day with repeat 1, settle the debt on the computer, sync, and see that the notification is gone after the app is opened; (6) deny the permission and see the message on the friend's page; (7) look at what the phone's battery saving does to a reminder at the next morning's time.

- **Tests.** The app has 1,077 (1,059 after batch 2), the command line 140, the user interface 204 (192): the code (round trip, 40 refused forms, the fuzz run, preview, accept and its refusals, conversion), the reminders (days, live and quiet, archived friends, bad input), the four commands through the mock runtime, the same code taken in on two devices and synced, the reminder logic of the page and its texts, and the schemas the Rust side sends.

**After the first look (2026-10-17).** The bill dialog has one field for who goes on a bill: the friends who are not on it yet, narrowing as the person types, and a typed name that is nobody's is offered as **add "name" as a friend**. A new name always becomes a friend (made at once, which leaves the friend behind if the dialog is cancelled), and their part is a debt when the bill is saved; **the screens no longer make guests** (the commands and a bill that has a guest are as before). The names of the people on a bill are set like the labels of the form, in grey, and the documents area of the transaction form is behind a switch like the split: **attach documents** (open and fixed open when the transaction has documents).

Also after the first look: the **split a bill** button beside **add** on the transactions and recurring pages is gone (the form's switch and the **split** button on each row are enough); the debts page and a friend's page keep theirs, which open a new transaction with the split on. The share mark `÷N` is plain text beside the amount on the list, and the toggle that opens what was paid and who has which part says **split details**, in the style of the **description** toggle under it. The document rows of the attach area are outlined rows with an icon, the name, the size and a button, and a file chosen for a new transaction no longer says "will be attached when you save" (its size is shown instead).

And again: the mark `÷N` is gone from both lists (everything is in **split details**). **Description** works the same way on both lists: the button keeps its name and the text opens under it; the buttons are a little quieter than the text they open. The recurring list has both toggles too: **split details** shows the proportions of the template in lowest terms (5000 and 5000 read as 1 and 1), and **description** shows the item's description, which the list did not show before.

And once more: the **split** and **edit split** buttons on the rows are gone (**edit** opens the same form), and the two toggles of the lists are one button, **details**, which opens a section **split** and a section **description**, each only when it is set. Both lists work the same. The description field of the forms says "optional" in grey until it is filled.

Found by use: a split set on a recurring item as 19 and 1 (amounts, so kept as 1900 and 100) opened again as 1900 and 100. A saved template now opens as **shares in lowest terms** (19 and 1), or as **percent with two decimals** when its weights make up the whole of a percent split (10,000 hundredths: 81.00 and 19.00). The proportions were always the same; only what the form shows changed, and a template that is opened and saved without a change is not written again.

**Categories and budgets are one page (2026-10-17).** The budgets page is gone: the categories page has the month's navigation, a side for expenses and a side for income, each with three totals (**allocated**, the budget of the side; **used**, what the categories' budgets add up to; **not used**, what is left of it), a framed list of the categories with their budget in the month, **edit** (the name and the budget from this month on, in a dialog) and **delete** (after a question), and below the **budget history**. The scroll areas are framed so they can be seen. The shortcut `g b` is gone.

**Questions before deleting (2026-10-17).** Every deletion asks, and the title of the question is `delete: ` and the name of the thing and nothing else: a transaction (every one now, not only a shared bill), a recurring item, a category, a document, an entry of a friend's ledger. The sentence that listed the people of a shared bill is gone from the question; their parts go with the bill as before. The question takes the focus when it opens, so neither **no** nor **yes** is outlined and Enter confirms nothing. In the budget history, **spent** is red when it is more than the expense budget, and **earned** is green when it is more than the income budget; a month with no budget is plain.

**The documents page (2026-10-17).** The page has one framed block at the top with how many documents there are and their size, the line about dropping files, and **add file** (it lights up when files are held over the window); each document is a card with an icon, its name, its date and size, the paper clip with the number of attachments and a chip **keep on phone**. The details dialog has the name, the date with a line saying it is the day on the paper, the box **keep a copy on my phone** (the sentence about marked and unmarked files is gone) and three titled sections: **about this file** (the kind in words: image, pdf file, text file, other file; and the size), **linked to** (with a plain sentence when it is attached to nothing) and **what you can do**, where **open**, **save a copy** and **delete** each have a line beside them saying what they do.

After a look at it: the line under the date, and the lines beside **save a copy** and **delete**, are gone (only **open** keeps its line), the sentence for a document attached to nothing is "not attached to anything yet.", and a dialog that has no field no longer opens with its first button (**back**) outlined: the panel takes the focus.

And: the document cards are as wide as the search and the type beside it (the list keeps no gutter on its right, only the width of its scroll bar), and in the sidebar **documents** comes right after **debts**.

And: the details dialog of a document has no section of actions any more (no **what you can do**, no **save a copy**, no **delete**); **open** stays under **about this file**. The card in the list has **details**, **save** (a copy, where the person chooses; not on a phone) and **delete**, in that order.

