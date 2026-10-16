# Debts and expense splitting design (phase 10)

Status: design written on 2026-10-16, nothing built yet. The decisions come from phase 10 of [PLAN.md](PLAN.md) and from the talk with the maintainer on the same day; this file says exactly how they work, so the code can follow it and a reviewer can check it. Section 16 lists what is still open.

## 1. What it has to do

- Keep a list of **friends**: a name and nothing else (plus an optional reminder, section 11). Nothing is imported from the phone's contacts.
- **Split a bill**: an expense that you paid is shared with some people, equally, by percent, by shares or by exact amounts, starting from an equal split. The transaction stays what you paid; **your expenses, budgets and statistics show only your part**.
- **Split a recurring item** too (rent with a flatmate): the item carries a template, and each payment is split by itself.
- Show **who owes whom**: for each friend a balance (positive: they owe you), made of the parts of split bills and of ledger entries you add by hand (a loan, a borrowing, a repayment).
- **Mark as paid** with an amount. An amount larger than the debt is accepted and turns the balance around (you now owe them the rest).
- **Share a bill as a QR code.** A friend who has the app scans it, sees the details in a dialog, and accepts or denies. Accepting makes a debt in their ledger and, if they leave the box ticked, an expense of their part.
- **Remind** you of a debt on a date you choose, at a time of day you choose, differently on the phone and the desktop.
- It follows the rest of the app: sync, currency change, backup, exports, the command line, "Delete all my data".
- Nothing opens a network connection. A QR code carries the bill itself.

Not goals: a chat or any messaging between people (it would need a server that someone runs; people already have one), importing contacts, storing anything about a friend but the name (no phone number, email or address), accounts or logins for friends, any server, an automatic message to a friend, knowing whether a friend paid (you mark it), more than one currency per ledger, splitting income, debts with interest or a schedule, a pending state kept on the receiver's device (a scanned bill is accepted or dropped on the spot).

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Person | A friend: name, archived or not, optional reminder | Encrypted database (`people`) |
| Split line | One friend's (or one guest's) part of a bill | `transaction_split_lines`, a child of a transaction |
| Ledger entry | A loan, a borrowing or a repayment with one friend | `ledger_entries` |
| Split template | The proportions of a recurring item's split, copied into each payment | `recurring_split_lines` and `recurring_transactions.split_my_weight` |
| Cost view | What a transaction costs you: the amount minus the lines | SQL view `transaction_costs` |
| Balance view | What a friend owes you, or you owe them | SQL view `person_balances` |
| Bill code | A QR code (or its text) holding one friend's part of one bill | Made on the screen, stored nowhere |

Your own part is **never stored**. It is `amount − sum of the lines`. That is the one place it exists, so no rounding or sync can make the parts disagree with the total.

## 3. The numbers

All amounts are integers in minor units, as everywhere.

- **Cost of a transaction** = `amount_minor − sum(its lines)`. An unsplit transaction costs its amount. Between 0 and the amount.
- **Balance of a person** (positive: they owe you) = the sum of their split lines + the signed ledger entries:

| Entry kind | Meaning | Effect on the balance |
|---|---|---|
| `LENT` | I lent them money | + amount |
| `BORROWED` | I borrowed from them | − amount |
| `REPAID_TO_ME` | They paid me back | − amount |
| `REPAID_BY_ME` | I paid them back | + amount |

A split line counts like a `LENT` entry dated the day of the bill. The balance is a running total: a repayment is not matched to a bill. "Anna owes you 25 and Bob 10" is the whole picture, and "this dinner is still unpaid" is not tracked. (The ledger of a person lists lines and entries together by date with the balance after each, so the story is readable.)

- **Owed to you** is the sum of the positive balances, **you owe** is the sum of the negative ones (as a positive number). Neither is income or expense, and neither touches a budget.

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

- **A guest** is a name typed on a bill for someone you do not track: their part is taken off your cost and no debt is made. To track them later, edit the bill and replace the guest by a friend.
- **Triggers**: stamps, deletion markers (ledger entries only), `updated_at` and the change history, the same as the other tables, so no way of writing data can forget one. The history (`audit_log`) records people, ledger entries and the lines of a bill like the other financial tables.
- **A line is only for an expense**: a trigger refuses a line on a transaction, or a template line on a recurring item, whose category is `INCOME`. A template line is refused when the item has no `split_my_weight`.
- **The parts never exceed the bill**: a trigger refuses a line, or a change of `amount_minor`, that makes the sum of the lines larger than the transaction's amount. (Like the `updated_at` triggers, the checks stand aside while a sync, a restore or a currency conversion is writing, because they write the pieces in some order; each of them checks the finished state afterwards, and `PRAGMA integrity_check` is joined by a query that finds any transaction whose lines exceed it.)
- **Changing a line changes its parent's stamp.** A trigger on the lines makes a no-op `UPDATE transactions SET name = name` for the parent (and the same for a template line and its recurring item; the stamp, `updated_at` and history triggers of `recurring_transactions` are recreated to watch `split_my_weight`), which the stamp trigger sees (`UPDATE OF` fires for a column named in the statement), so a split edited on one device is a newer transaction on the next sync. A test fails if a change to a line leaves the stamp as it was.
- **Views**: `transaction_costs(transaction_id, cost_minor)` and `person_balances(person_id, balance_minor)`, as defined in section 3, with an index on `transaction_split_lines(transaction_id)` and on `(person_id)`, and on `ledger_entries(person_id)`.

## 5. Splitting a bill

**The dialog.** On a new or existing expense a switch "Split this bill" opens the participants: you (always first) and any number of friends or guests. The bill starts as an equal split. A mode selector changes how the numbers are typed:

| Mode | You type | Rule |
|---|---|---|
| Equal | nothing | Everyone has weight 1 |
| Percent | each person's percent, two decimals | Must add up to exactly 100.00 |
| Shares | a whole number for each (2, 1, 1) | At least one is above 0 |
| Exact | each person's amount | Must add up to exactly the bill |

A footer shows "total, left to assign", and Save is off until nothing is left. A button "give the rest to me" fills your row. A weight of 0 is allowed (you paid for someone's lunch: your row is 0); a friend with 0 is dropped when the bill is saved.

**One function.** `splits::allocate(total, weights)` in Rust turns weights into amounts that add up to the total. For each weight `w` of the sum `W`: the exact quota is `total × w / W` (in 128-bit integers); take the floor; the cents that are left go one each to the largest remainders `(total × w) mod W`, ties to the lower position (you are position 0). So 100.00 among three is 33.34 (you), 33.33, 33.33, and the parts differ from the exact quota by less than a cent. The dialog asks Rust for a preview on every change (`split_preview`, a local command): there is **no copy of the rule in the interface**, so what the preview shows is what is stored.

**What is stored.** The amounts of the friends' and guests' lines, nothing else: not the mode, not the percentages. Opening a saved bill shows the Exact mode with the amounts, and the other modes start from them (percent shows each amount's percent to two decimals) or "start again from equal". This loses a typed "40 / 60" after saving and nothing else, and it spares a mode and weights that would have to agree with the amounts through every sync and conversion.

**Editing.** Changing the amount of a split transaction rescales the parts: `allocate(new total, [old parts of every person, yours included])`, shown in the dialog before it is saved. Clearing the split deletes the lines. Changing the date or category changes no line. **Deleting** a split transaction removes its lines, which changes the balances of the people on it; the confirmation lists them ("Anna: 25 less owed to you"). A repayment already marked stays, so a balance can become negative: you were paid for a bill that no longer exists, and the app says what you now owe.

**Recurring items.** A recurring item can carry a **split template**, so each payment is split by itself. The template is kept as **weights**, not amounts, because a payment can differ from the plan (an electricity bill) and an item can have no fixed amount: your weight in `split_my_weight` and one weight per friend or guest. Equal gives everyone 1, shares are the numbers typed, percent is in hundredths (10,000 in all), and exact amounts are kept as weights too, so a payment of the planned amount gives exactly what was typed and any other payment keeps the same proportions. When the item is paid (`recurring::pay`) the transaction is made and, in the same database transaction, `allocate(amount, weights)` makes its lines (a part of 0 is dropped). From then on it is an ordinary split transaction: editing it changes only it. **Editing the template changes the payments to come and never the ones already made**, as editing the amount of an item already works. A friend who is archived keeps their place (the debt goes on growing); the recurring page marks such a template so it can be changed. The dialog is the bill dialog; for an item with no amount it shows percentages instead of amounts. Income items cannot have a template. An item without a template makes unsplit transactions, and any of them can be split by hand afterwards.

## 6. The ledger

- **Entries by hand**: I lent, I borrowed, they paid me back, I paid them back. Each has a friend, a date, an amount, a note, an optional link to a transaction and optional documents (a loan note, an invoice).
- **Mark as paid** on a friend opens a dialog for the amount, which starts as the whole balance, with the date and a note. It adds `REPAID_TO_ME` when they owe you and `REPAID_BY_ME` when you owe them. An amount **larger than the balance** is accepted: the dialog says first "this is 15.00 more than Anna owes you; the rest will count as you owing Anna", and afterwards the balance has the other sign. When a balance reaches zero the friend's reminder is cleared.
- **I borrowed, with an expense.** The "I borrowed" form has a box "this was my share of a bill: add it to my expenses", with a category. Ticked, it makes the expense (name, your amount, the date, the category) and the `BORROWED` entry linked to it in one database transaction. This is the case "someone else paid": the dinner costs you your part in the month, and what you owe is a debt. Paying them back later is a `REPAID_BY_ME` entry and never a second expense. The scanned bill (section 10) ends in the same function.
- **Entries never touch the transactions.** Editing or deleting the linked expense does not change the entry (deleting it only clears the link), and editing an entry does not change the expense. An entry typed by hand is never changed by a split.
- **Never income, never in a budget.** No query that sums income or expense reads `ledger_entries`.

## 7. Every sum uses the cost

The rule: **no query adds up `transactions.amount_minor` for a figure about what you spent or earned; it adds `transaction_costs.cost_minor`.** The queries found on 2026-10-16 (rechecked when building):

| Where | What | Uses the cost |
|---|---|---|
| `budgets.rs`: `actual_minor`, the monthly history | Spent and earned against a budget, the history table | yes |
| `stats.rs`: the category actuals, the monthly totals, income and expense per month, the largest categories | Dashboard, trends, category breakdown | yes |
| `stats.rs`: `commitments` | What the active recurring items add up to in an average month | your part of the planned amount: `allocate(amount, weights)`, position 0 |
| `stats.rs`: planned against paid | The bill against its plan | **no**: both sides are the whole bill, because it is about how the price moved and a split changes none of that. On the guard's list with that reason |
| `report.rs`: the category totals | The PDF report | yes |
| `export.rs`: the monthly summary and the totals sheet | JSON, CSV and Excel summaries | yes |
| `export.rs`: the list of transactions | JSON, CSV and Excel rows | both: `amount` (paid) and `my_share` |
| The API and the command line | Every route that returns a transaction or a total | `my_share` next to `amount`; totals use the cost |
| The screens | The transaction list shows your part as its amount, with a mark (`÷4`) and the full amount and the names in the detail | the cost |

**The guard.** A test reads the SQL strings in `src-tauri/src` and fails when one of them sums `amount_minor` of `transactions` (or reads it for a total) and is not on a short list with a reason each (the definition of the view itself; planned against paid; the planned amounts of recurring items, whose part of yours is worked out by `allocate`; backup, sync and currency code, which move rows and total nothing). A new query that forgets the split fails here. Then **random tests**: for random bills, every figure of every module equals a plain recomputation that uses the parts.

**Speed.** The view's subquery runs per transaction through an index. The benchmark of `cargo run --release --example bench` (60,000 transactions) gets a split on one in five, and every list, search, budget and statistics query must stay under 40 ms. If it does not, the sums join a pre-summed table of lines instead.

## 8. Currency change

The conversion that already converts transactions, recurring items and budgets (`currency.rs`, one rate, rounded half up) also converts the new rows, in `change` and in `apply_recorded`, so both devices work out the same:

- **Ledger entries**: each amount converted, at least one cent.
- **A split bill**: the new amount is converted as now; the parts, yours included, are then **re-allocated** to that amount with the old parts as the weights (`allocate(new amount, [old parts and yours])`). A part that comes out as 0 is dropped. So the lines still add up to the amount to the cent, and your part is never negative. A plain rounding of every line could make the lines add up to more than the bill, which is why it is not used.
- **A split template** holds proportions, so a currency change leaves it alone (the item's own amount is converted as now).
- **Balances** are sums of converted pieces, so they may differ from the converted old balance by a few cents; every device has the same number. The conversion does not change a stamp, as for the rows that exist.

## 9. Sync

The same rules as `docs/sync-design.md` (a comparison of lists of `(sync_id, stamp)`, newer whole row wins, stamps of derived changes from stamps and not from the clock). Protocol 3 becomes **4** and the schema is 12, so an older app is told to update.

- **`people` and `ledger_entries`** are tables of the ordinary kind: `sync_id`, `stamp`, hashes and digests, rows, conflicts shown in the review dialog (the facts shown: name and archived for a person; friend, kind, amount, date and note for an entry). Ledger entries leave a deletion marker when deleted.
- **The lines of a bill travel inside its transaction's row**, as a list `[{p: person sync_id or null, g: guest name or null, a: amount}]` in the row. A transaction that wins replaces all of its local lines with the list it brought (an empty or missing list removes them). So the lines are one unit with the transaction, and two devices that edit the same split can never end with parts from both: the newer whole transaction wins. Lines have no marker, and a deleted transaction takes them with it.
- **The template of a recurring item travels inside its row** the same way: `split: {mine: w, lines: [{p, g, w}]}`, or none. The recurring row that wins replaces the local template.
- **Applying order**: shared settings, categories, **people**, recurring items, budgets, transactions (with their lines), **ledger entries**, documents, document links, then deletion markers.

Merge rules for the new cases:

| Case | Rule |
|---|---|
| Person edited on both (name, archived, reminder) | Newer whole row wins. A real conflict goes to the review dialog |
| Person with the same name, both active, on both | The lower `sync_id` is kept. The other is marked deleted with `merged_into`; the transactions and recurring items with a line for it and the ledger entries move to the kept one, with the `moved` stamps the categories use. If a bill would then hold two lines for one person, they become one line with the amounts added (a template: the weights added) |
| A rename onto a name another person is giving up, a swap of names | The same waiting and temporary names as categories |
| Entry added on both | Both kept: different `sync_id`s. Two with the same `origin_id` are the same scanned bill: the newer stays, the other becomes a deletion marker |
| Entry edited on both, or edited on one and deleted on the other | Newer wins (a marker has a stamp) |
| A split edited on both, or a transaction edited on one and its split on the other | The newer whole transaction wins, with its lines |
| A recurring item or its template edited on both | The newer whole row wins, with its template. A payment made on both devices keeps the existing rule (one transaction stays, with its own lines, the other becomes a marker) |
| Transaction deleted on one, an entry linked to it on the other | The entry stays and its link is cleared by the foreign key on both, with no stamp (the same as the link from a transaction to a recurring item) |
| Document linked to a ledger entry | The rules of section 10 of the documents design: the link is a row; it goes with its entry; a link for an entry this device knows as deleted is not taken and is no error |
| A person archived on one device while the other adds a line or an entry for them | Both are kept. Archiving hides a person and removes nothing |

Row checks on arrival (the screens' own rules): name rule, closed set of kinds, amounts in range, real dates, note length, `origin_id` of 32 hex characters, a reference to a person or transaction that is unknown and not a known deletion stops the sync; on a transaction: lines at most 30, each exactly one of person and guest, no person twice, **the lines add up to no more than the amount**, and the category is an expense; on a recurring item: a template of at most 30 lines with weights in range, at least one above 0, and an expense category. A row that fails ends the sync and names the table, never the content. The currency generations cover the new rows (section 8). A sync summary line counts people and entries.

## 10. Bill codes (QR)

**Who sees what.** For each friend (not a guest) on a split bill there is "show code": a QR code of that friend's part. The same text can be copied ("copy code"). Nobody types a code of digits as in a pairing, and nothing is encrypted or signed: the code is a notice, and the friend decides whether to accept it.

**The text.** `MNB1:` followed by base64url of a JSON object (a different prefix from the pairing code's `MNY1:`, so a scanner can tell them apart and say "this is a pairing code; use sync"):

```
{"v":1, "id":"<32 hex>", "from":"<name, 1..50>", "what":"<1..60>", "date":"YYYY-MM-DD", "amt":2500, "total":10000, "cur":"EUR"}
```

- `amt` is the friend's part, `total` the whole bill (so the dialog can say "25.00 of 100.00"). No other person's name or part is in it.
- `id` is the first 16 bytes of `SHA-256("meowny-bill-1" ‖ transaction sync_id ‖ person sync_id)` in hex: the same code every time it is shown, and it gives away no id of the sender's.
- `from` is the name the sender gives, asked when the first code is shown and remembered on that device. It is not synced.
- At most 1 KiB. It is a snapshot: later edits of the bill change nothing on the friend's side, and the friend's side tells the sender nothing.

**Scanning.** On the phone the scanner of the sync (`ui/src/api/scanner.ts`) opens from "scan a bill" on the debts page. The desktop has no scanner: its debts page has "paste a code". Both lead to one Rust function, `bill_preview(text)`, which parses strictly (size, prefix, version, exact fields, lengths, no control or direction characters, `amt` and `total` in range, `amt ≤ total`, a real date, a three-letter currency) and returns the details. **Nothing is written by a scan.** A malformed or unknown version says so and stops.

**The dialog.** It shows, as plain text (never as markup): who it is from, what, the date, "your part 25.00 EUR of 100.00", and these choices:

- **Which friend**: a list of friends with the one whose name matches `from` (ignoring case) already chosen, or "new friend" with the name filled in and editable.
- **Add to my expenses**: ticked by default, with an expense category to choose (the one used last for this friend is suggested).
- **Currency**: when `cur` is not your currency, the dialog asks for the rate ("1 EUR = ___ USD", the same rate rules as a currency change) and shows the converted amount, which can be edited. Accept stays off until there is one. Nothing is fetched.
- **Accept** and **Deny**. Deny closes it and stores nothing. A code that was already accepted (an entry with this `origin_id` exists) is not accepted again: the dialog says so.

**Accept** is one database transaction (`bill_accept`): the friend is created if new; the expense is made when ticked (the bill's name, your converted part, the bill's date, the chosen category); and the `BORROWED` entry with the converted part, the bill's date, a note (`25.00 EUR at 1.08` when converted) and the `origin_id`, linked to the expense. The same function serves "I borrowed, with an expense" (section 6).

**What this does not do.** The sender does not learn that the code was scanned or that the friend paid. Each side marks payments itself, and each side has its own reminders. A friend who corrects nothing can ask the sender to show a changed code; the old entry is deleted and the new code accepted (a deleted entry frees its `origin_id`). A code that reaches the wrong person only tells them a name, a bill and a sum, which is why it is shown only while the dialog is open and the phone's window is secure.

## 11. Reminders

A reminder belongs to a friend: `remind_on` (a date) and, optionally, `remind_every_days`. It is **live while the balance is not 0**: the dates are `remind_on`, then every N days if N is set. Nothing is written when a reminder fires, so two devices never fight over it; each keeps on its own device (webview storage, like `due-notified`) which date it has shown. The **time of day** is a setting of the device (09:00 unless changed). Settling a friend to 0 clears their reminder.

The text is the same for everyone, **"meowNY: a debt reminder"** with a count: never a name or an amount, because the app is locked when it shows and a notification can be seen on a lock screen and kept in the system's history. The debts page then shows which friends are due.

- **Desktop.** The daily notification of due recurring items (`useDueNotifications`) is extended. While the app is open and unlocked, at the chosen time or after it, one notification for the reminders that are due and not yet shown. A reminder that came due while the app was closed is listed on the debts page and in one notification at the next unlock. The settings say plainly that the desktop reminds only while meowNY is open. The existing switch "notifications" turns these off as well.
- **Phone.** The phone has no notifications today. For every friend with a live reminder the app schedules the next four occurrences with the system (so they fire with the app closed), asks for the notification permission the first time a reminder is set, and schedules again at each unlock, after each sync and when a reminder changes. Because the app is locked when they fire, the text is fixed when they are scheduled. A friend settled by a sync on a phone that is not opened keeps their scheduled notifications until the next unlock, which can show a reminder for a debt that is gone; that is accepted and said in the settings. **Not yet known** until a real phone has run it: whether the notification plugin can schedule on Android, the permission and the exact-alarm rules of Android 13 and later, and how it behaves with the window secure. These are the spike of section 15.

## 12. Backup, restore, exports, "Delete all my data"

- **Backup and restore**: `people`, `ledger_entries`, `transaction_split_lines` and `recurring_split_lines` join `TABLES` in `backup.rs`. Restore validates the rows by the rules of section 9 (lines against their bill, references, kinds) and refuses the file whole when one fails. A restore gives the device a new id and unpairs it, as now.
- **"Delete all my data"** removes the database, which holds all of it. Nothing of the ledger is anywhere else: the bill codes are not stored.
- **Exports.** JSON, CSV and Excel gain people, ledger entries and the lines of every bill; transaction rows have `amount` and `my_share`. A limited export includes the people its transactions mention and no ledger entries unless the person ticks "debts". The summary of who owes what is saved as **text, CSV or PDF**, for you to send yourself: a balance for every friend, or the **statement of one friend** (their lines and entries by date and the balance). The app opens no connection to anyone.
- **The monthly PDF and the summaries** use the cost (section 7).

## 13. Command line and API

New routes, named like the existing ones and given a command by construction through `cli/schema.json` (a test fails when they differ): read: people, a person's ledger, balances; write: add, rename, archive and set the reminder of a person; add, edit and delete a ledger entry; mark as paid (`settle`); set and clear the split of a transaction (`split_set` takes the mode and the values, the app allocates); preview a split; the recurring routes take and return a split template (as the mode and values, kept as weights). The transaction routes return `my_share` and the split beside `amount`. The scan and the acceptance of a bill code, the display of a code, and the notifications are not in the API: they need the person at the app. Write routes need a write token, like the others.

## 14. The screens

Desktop and phone, in all seven languages (the i18n key test covers them).

- **Debts page** (sidebar on the desktop, an icon in the top bar of the phone): "owed to you" and "you owe" at the top; the friends with their balance, the largest first; per friend "mark as paid"; "I lent", "I borrowed", "add friend"; "show removed" for archived friends; "scan a bill" (phone) and "paste a code" (desktop). Due reminders appear as a line above the list.
- **A friend's page**: the name, the balance, the reminder (date, repeat), the ledger with lines and entries by date and the balance after each, documents, "statement".
- **The bill dialog** (part of the transaction form): section 5. After saving, each friend line has "show code".
- **The recurring form** has the same split switch and dialog, and the recurring list marks the items that are split (and the ones whose template names an archived friend).
- **The transaction list**: your part is the amount; a mark `÷4`; the detail shows what was paid and who has which part.
- **The dashboard** gets a small card with the two numbers, which opens the debts page.
- **Settings**: the reminder time of this device, and the plain statement of what the desktop and the phone can do.
- **The review dialog of a sync** and the sync summary know the new rows.

## 15. Threats and what stops them

| Threat | What stops it |
|---|---|
| A code or pasted text built to break the parser, or very large | One strict parser with a size limit, fixed fields and a version; no allocation from a length in the text; a fuzz test; nothing is written before Accept |
| Someone shows a code that says "you owe me 500" | The friend sees the details and decides; no inbox, no default; Deny stores nothing |
| Markup or tricks in names (`from`, `what`) | Shown as text only; control and direction characters are refused as everywhere else |
| The same code accepted twice | `origin_id` is unique; the dialog says it was already added |
| A code read from your screen | Shown only while a dialog is open; the phone's window is secure; a code holds no other person's data |
| A notification shows a debt on a lock screen | The text is fixed and holds no name or amount |
| Sums that forget the split | One view, the guard test of section 7, random tests against a plain recomputation |
| Parts that disagree with the bill (rounding, conversion, two devices) | Your part is derived; one `allocate`; the lines travel with their transaction; a check on every row from outside |
| A damaged or hostile sync or backup row | The checks of section 9 end the sync or refuse the file, naming the table |
| A split deleted by mistake changing balances | The confirmation lists the people and amounts |

## 16. Open points

- **A bill code for a loan** (not tied to an expense): the text has a version and room for a `kind`; not built.
- **A code back** ("I paid you") so the sender's side can mark a payment: not built.
- **The phone's reminders** (section 11) wait for a real phone.
- **The paste box on the desktop** is a proposal; it can be dropped if "scan only" is preferred.
- **Several currencies for one friend** are not supported; the ledger is in the app's currency.
- **Amount filters of the transaction list** keep using the paid amount; the list shows both.
- A review by someone else of the bill-code parser before release, with the sync's and the documents' formats.

## 17. Tests

- **Allocation**: property tests with random totals, weights and counts: the parts add up to the total, each is within a cent of its exact quota, the result does not depend on anything but the weights and positions, a zero weight gets zero; fixed cases (100.00 among three; 0.01 among two; one huge weight); percent that does not add up to 100.00 and exact amounts that do not add up are refused.
- **Cost and balance**: the views against a plain recomputation on random bills, entries, edits and deletes; a repayment larger than the debt turns the balance around; deleting a bill changes the balances it should; archiving changes none.
- **Every sum uses the cost**: the guard of section 7, and random bills run through every module (budgets, statistics, report, exports, API, CLI) against a recomputation.
- **Never income**: nothing in the ledger appears in income, in a budget or in a statistic.
- **Rows**: constraints and triggers for the three tables (kinds, ranges, one line per person, the lines never above the bill, no line on income, stamps, markers, audit).
- **Templates**: paying a recurring item makes lines that add up to the payment, for the planned amount (exactly what was typed), for another amount, and for an item with no amount; editing a template leaves the payments made alone; no template on income; the average-month figure uses your part; the same occurrence paid on two devices ends with one transaction and its own lines; a merged friend's template line moves; the weights survive a currency change unchanged.
- **Currency**: a change keeps every bill's lines adding up to its amount, drops no one but a part that becomes 0, converts entries, gives the same result on both devices, and keeps balances within the rounding.
- **Sync**: every case of section 9 has a test; random edits of people, entries and splits on two devices from a fixed seed converge (the existing test extended); a split edited on both ends with one side's parts only; the same name on two devices merges and moves everything; a second sync finds nothing; a hostile row (lines above the bill, an unknown person, an income bill) stops the sync.
- **Bill codes**: round trips; every malformed input refused (wrong prefix, version, length, fields, dates, amounts, control characters) and a fuzz run that never panics; a scan writes nothing; Accept writes the person, the expense and the entry together or nothing; the same code twice; a different currency with a rate; a code never holds another person's name or part.
- **Reminders**: which dates are due for once-only and repeating reminders, none for a balance of 0, the notification text never holds a name or amount; the phone's scheduling by hand.
- **Backup**: a round trip with all three tables; a hostile file refused whole. "Delete all my data" leaves no table.
- **Screens**: the bill dialog (every mode, the footer, the preview), the debts page, the friend's page, the mark-as-paid dialog (including the larger amount) and the bill-code dialog looked at in a browser at desktop and phone width with the commands faked.
- **By hand**: the "done when" of the plan, on a desktop and a real phone, with a second phone for the code.

## 18. Batches

Three, each one commit:

1. **The numbers.** This design, migration `0012`, `splits.rs` (`allocate`, the dialog's preview), people, ledger, mark as paid, the borrowing with an expense, the split template of a recurring item and its payment, the views and the cost in every sum with the guard test, the currency change, backup and restore, the exports and the summary, the API and the command line, and their tests. No screens.
2. **The screens and the sync.** The debts page, a friend's page, the bill dialog (and its use in the recurring form), the list marks, the dashboard card, documents on entries, the seven languages, then protocol 4 with the merge rules and the convergence tests, and the phone.
3. **Bill codes and reminders.** The code, the scan and paste, the dialog with conversion, accept and deny, the reminders on desktop and phone, then the user guide, the data inventory and the real-phone checks.
