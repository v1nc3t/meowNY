# Debts and expense splitting design

How friends, split bills, the ledger, bill codes and reminders work. Section numbers are cited in the code; keep them.
User side: the [guide](https://v1nc3t.github.io/meowNY/guide.html#debts).

## 1. What it has to do

Friends are names only. A bill you paid can be split; your expenses, budgets and statistics count only your part. A
recurring item can carry a split template. A balance per friend comes from split parts and ledger entries; mark as
paid takes any amount, also more than the debt. A bill can be shown to a friend as a QR code. Reminders. Everything
follows sync, currency changes, backups, exports and the CLI. No network. Not goals: messaging, contacts, accounts for
friends, knowing whether a friend paid, several currencies per ledger, splitting income.

## 2. The pieces

| Name | Where |
|---|---|
| Person (name, archived, `merged_into`) | `people` |
| Split line: one friend's or guest's part of a bill | `transaction_split_lines`, child of a transaction |
| Ledger entry: a loan, borrowing or repayment | `ledger_entries` |
| Split template of a recurring item | `recurring_split_lines` and `recurring_transactions.split_my_weight` |
| The friend who paid a bill or a template (none: you) | `transactions.paid_by`, `recurring_transactions.split_paid_by` |
| What a transaction costs you; what a friend owes | Views `transaction_costs`, `person_balances` |
| Bill code | Made on screen, stored nowhere |

Your own part is never stored: it is the amount minus the lines, so parts can never disagree with the total. A friend's own part
is a line like any other, also when they paid.

## 3. The numbers

Integers in minor units. Cost = `amount_minor - sum(lines)`, whoever paid. Balance (positive: they owe you) = their lines on
bills you paid + `LENT` + `REPAID_BY_ME` - `BORROWED` - `REPAID_TO_ME` - the cost of each bill they paid (you owe them your
part; the others on that bill owe the payer, not you, so their lines count for nothing here). A balance is a running total,
not matched to bills, and never income, expense or part of a budget.

## 4. The rows

Migration `0012_debts.sql` (`0013` for deleted expenses, `0017` for the payer). Names 1 to 50 characters, unique among active people
ignoring case; people are archived, never deleted. Entries: kind, amount, date, a note of at most 200 characters, an
optional expense (`ON DELETE SET NULL`) and an `origin_id` from a bill code (unique). Lines: exactly one of person or
guest, at most 30 per bill, one per person. Triggers refuse lines on income and parts above the bill (they stand aside
while a sync, restore or conversion writes; `splits::check` checks the result). Changing a line gives its parent a new
stamp. `document_links` gained a ledger entry target. `0017` adds `transactions.paid_by` and
`recurring_transactions.split_paid_by` (a person, `NULL` for you), puts them in the triggers that stamp a row and set
`updated_at`, extends the refusal of income to a bill with a payer, and makes `person_balances` again.

## 5. Splitting a bill

Equal, percent (exactly 100.00), shares, or exact amounts (exactly the bill). One Rust function, `splits::allocate`,
turns weights into amounts that add up: floor of each quota in 128-bit integers, the odd cents to the largest
remainders, ties to the lower position (you are 0). The screen asks Rust for every preview; the interface has no copy
of the rule. Only the friends' amounts are stored, so a saved bill opens as exact amounts. A new amount rescales every
part. A recurring item keeps weights; each payment is allocated when paid, and editing the template never changes past
payments. A bill is paid by you or by one friend (`paidBy`). The payer must be one of the people on the split, and your part
must be above 0 (a bill a friend paid with nothing of yours is not your expense); the payer's own part may be 0. The cost
stays your part. A template has the same payer, and each payment copies it. Taking the split off clears it.

## 6. The ledger

Lend, borrow, paid back to me, paid back by me, each with friend, date, amount, note and optional documents. Mark as
paid starts at the whole balance and says first when an amount turns the balance around. "This was my share of a
bill" makes the expense and the `BORROWED` entry together; paying back later is an entry, never a second expense.
Nothing that sums income or expense reads the ledger.

## 7. Every sum uses the cost

No query adds `transactions.amount_minor` for what you spent or earned; it adds `transaction_costs.cost_minor`. Budgets,
statistics, reports, export summaries and the API all use the cost; transaction rows carry both `amount` and
`my_share`. Planned-against-paid compares whole bills on purpose. A guard test reads the SQL in `src-tauri/src` and
fails on a new sum of `amount_minor` that is not on its short list with a reason; random tests recompute every figure.

## 8. Currency change

The conversion of `currency.rs` also converts ledger entries (at least one cent) and re-allocates a split bill's parts
to its converted amount, with the old parts as weights, so they still add up. Templates are proportions and stay.
Both devices get the same numbers.

## 9. Sync

Protocol 8 (4 brought these tables, 8 the payer). `people` and `ledger_entries` are ordinary synced tables. A bill's lines
travel inside its transaction's row, and the winning row replaces all local lines; a template travels inside its item. The
payer travels as the person's `sync_id` in the bill's row and in the template (`paidBy`), and a merged friend's bills and
templates move to the one kept. Same-name active friends merge (lower `sync_id` kept, rows moved, two lines for one friend
added). Two entries from one bill code (`origin_id`) become one. Every arriving row is checked, including that lines never
exceed the bill; a failure ends the sync naming the table.

## 10. Bill codes (QR)

`MNB1:` plus base64url of `{"v":1,"id","from","what","date","amt","total","cur"}`: one friend's part, the whole bill,
your chosen name, and nothing about anyone else, except that when a friend paid the code also holds the payer's name (`by`,
left out when you paid): the friend's debt goes to the payer, so the dialog offers that friend first, and the payer gets
no code. `id` is the first 16 bytes of `SHA-256("meowny-bill-1" || transaction sync_id || person sync_id)`. At most 1 KiB,
a notice, not signed or encrypted. One strict parser (`debts/codes.rs`) refuses anything not exactly that, including
control and direction characters.
A scan writes nothing; the dialog shows the details, the friend to use, "add to my expenses" and, for another
currency, a typed rate (nothing is fetched). Accept (`bill_accept`) makes the friend, expense and entry in one
transaction, or nothing. The sender learns nothing.

## 11. Reminders

One setting per device (never, every week, 2 weeks or month) for every friend whose balance is not 0: the first that
long after their latest line, then every period. Worked out from the statements (`debts/reminders.rs`); nothing new is
stored and nothing is written when one comes due. The friend's own `remind_on` and `remind_every_days` are still
stored and synced but nothing reads them. The text is "meowNY: a debt reminder" and a count, never a name or amount.
The desktop shows them while open and unlocked; the phone hands the system the next four days (inexact alarms).

## 12. Backup, restore, exports, "Delete all my data"

All four tables are in backups; a restore checks them by the section 9 rules and refuses the file whole on a failure.
JSON, CSV and Excel include people, the ledger and the parts (a limited export names only the friends on its bills); a part
names the friend who paid its bill when it was not you (JSON and Excel; the CSV of transactions has no parts).
A summary or one friend's statement is saved as TXT, CSV or PDF for the person to send. Bill codes are never stored.

## 13. Command line and API

Read: people, a person's ledger, balances, split preview. Write: people, ledger entries, settle, set or clear a
transaction's or recurring item's split (a split takes `paidBy`, a friend's id, and a transaction or template shows
`paidBy` as `{personId, name}` or null). Exporting debts needs the person's yes. Bill codes and notifications need the
person at the app and are not in the API.

## 14. The screens

A debts page (friends, due payments, history of debts; lend, borrow, split a bill, scan or paste a code), a friend's
page with the ledger and statement, the split switch and dialog in the transaction and recurring forms, **details** on
list rows, a dashboard card, and the reminder setting. Seven languages.

## 15. Threats and what stops them

A hostile or huge code: one strict parser, a fuzz test, nothing written before Accept. "You owe me 500": the friend
decides; Deny stores nothing. Markup in names: shown as text, control characters refused. A code taken twice: unique
`origin_id`. A code read off your screen: shown only in its dialog, the phone window is secure. A lock screen: fixed
text. Sums that forget the split: one view and the guard test. Parts that disagree: your part is derived, one
`allocate`, lines travel with their bill.

## 16. Open points

A code for a loan, and a code back ("I paid you"). Several currencies per friend. The ten-year budget history takes
about 50 ms with many shared bills (target 40). An outside review of the bill-code parser.

## 17. Tests

Allocation properties and fixed cases; the views against recomputation; the section 7 guard; constraints and
triggers; templates and payments; currency changes on both devices; every sync case and random edits; bill codes
(round trips, 40 refused forms, a 30,000-code fuzz run, accept and its refusals); reminders; backups; bills a friend
paid (balances, statements, codes, sync merges). Checked by hand on a real phone with a second phone for the code
(2026-10-23).
