# Accounts and transfers design

How accounts, balances, transfers and net worth work. Section numbers are cited in the code; keep them.
User side: the [guide](https://v1nc3t.github.io/meowNY/guide.html#accounts). Status: built (migration `0018`, sync protocol 9).

## 1. What it has to do

- An account is a place your money is: a bank account, savings, cash, a credit card. It has a name, a kind, an optional number, an
  opening balance with its date, an order in the list, and it can be archived.
- Every transaction, recurring item and ledger entry belongs to exactly one account. A transfer moves money between two of your
  accounts and is never income, expense, part of a budget or of a statistic.
- Each account has a balance on any day, a list of what moved it, and a chart over time. The dashboard shows net worth.
- A person who never wants accounts is not bothered: there is always one account (the main account), everything goes into it,
  and with one account no form shows an account field.
- Everything follows sync, currency changes, backups, exports, the demo data and the CLI. No network. No new dependency.
- Not goals: expense and revenue (counterparty) accounts, liabilities (the debts feature covers them), piggy banks, rules,
  reconciliation, bank import, a currency per account, an export limited to one account, splitting one transaction over accounts.

## 2. The pieces

| Name | Where |
|---|---|
| Account | `accounts` |
| The main account: local `id` 1, `sync_id` `00000000000000000000000000000001`, made by the migration on every device | a row of `accounts` |
| The account of a transaction, a recurring item, a ledger entry | `transactions.account_id`, `recurring_transactions.account_id`, `ledger_entries.account_id` |
| Transfer (from, to, amount, date, note) | `transfers` |
| What moved an account, one row per movement | View `account_movements` |
| Balance, change this month, last activity, net worth | Worked out from the view, stored nowhere |
| Rust | `src-tauri/src/accounts.rs` and `src-tauri/src/accounts/transfers.rs` |

Constants in `accounts.rs`: `MAIN_ID: i64 = 1`, `MAIN_SYNC_ID: &str = "00000000000000000000000000000001"`,
`MAIN_NAME: &str = "Main account"`. Kinds: `CHECKING`, `SAVINGS`, `CASH`, `CREDIT_CARD` (Rust enum `Kind`, as `ledger::Kind`).

## 3. The numbers

Integers in minor units. One currency, the app's (`settings.currency`).

**What moves an account** (the view `account_movements`: `account_id`, `day`, `delta_minor`, `source`, `source_id`):

| Source | Day | `delta_minor` |
|---|---|---|
| `OPENING`: the opening balance, when it is not 0 | `opening_date` | `opening_balance_minor` (signed) |
| `TRANSACTION`, income | `payment_date` | `+amount_minor` |
| `TRANSACTION`, expense you paid (`paid_by IS NULL`), split or not | `payment_date` | `-amount_minor`, the **whole bill** |
| A split bill a friend paid (`paid_by` set) | | no row: no money left your account |
| `TRANSFER_IN` | `transfer_date` | `+amount_minor` on `to_account_id` |
| `TRANSFER_OUT` | `transfer_date` | `-amount_minor` on `from_account_id` |
| `LEDGER` `BORROWED`, `REPAID_TO_ME` | `entry_date` | `+amount_minor` |
| `LEDGER` `LENT`, `REPAID_BY_ME` | `entry_date` | `-amount_minor` |

- **Balance of account A on day D** = sum of `delta_minor` where `account_id = A AND day <= D`. Nothing else. The current balance
  is D = the app's today (`sync-design.md`, section 10).
- **The opening balance is a movement dated `opening_date`**, as in Firefly. Rows dated before it still count; no row is ever
  refused or ignored for its date. Before `opening_date` the balance simply does not hold the opening amount yet. Why: no refused
  save, no sync rule for "a row before its account opened", and an opening date typed too late only bends the chart.
- **Future-dated rows** count from their day, so they are not in today's balance. The chart and the statement end at today
  unless the period shown reaches further.
- **Split bills.** The account sees money that really moved, not your part: the whole bill when you paid, nothing when a friend
  paid. Your part (`transaction_costs.cost_minor`) stays what budgets and statistics count (`debts-design.md`, section 7).
  The rest of the money moves through the ledger: a friend who pays you back is `REPAID_TO_ME` (+), paying a friend back is
  `REPAID_BY_ME` (-). "This was my share of a bill" (`borrow_with_expense`) makes an expense (-) and a `BORROWED` entry (+) in
  the same account: together 0, which is right, the friend paid. This is why ledger entries carry an account: without it a
  balance would be wrong for ever after the first repayment.
- **Archived accounts** keep their rows and their balance. Archiving changes no number.
- **Change this month** = sum of `delta_minor` with `day` from the first of the current month to today.
  **Last activity** = the latest `day <= today` of a movement that is not `OPENING`; none when there is none.
- **Net worth** = `in_accounts + friends`: `in_accounts` is the sum of the current balance of every account, archived too;
  `friends` is `SUM(person_balances.balance_minor)` (what friends owe you minus what you owe). Money a friend owes you is yours.
- **Chart**: for a year, the balance on the last day of each month; for the current month, today; months after today have no
  point.
- A new figure never adds `transactions.amount_minor` in Rust: it sums `account_movements.delta_minor`, so the guard of
  `debts-design.md` section 7 has nothing new to allow.

## 4. The rows

Migration `0018_accounts.sql`, appended to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Triggers that read the clock use the form of
`0016` (`+ offset_ms`). Order inside the file: 4.1 to 4.7.

### 4.1 `sync_deleted` gets two kinds

Rebuild as `0012` did: drop the triggers that write to it (`trg_budgets_sync_delete`, `trg_recurring_transactions_sync_delete`,
`trg_transactions_sync_delete`, `trg_documents_sync_delete`, `trg_document_links_sync_delete`, `trg_ledger_entries_sync_delete`),
rebuild the table with `'accounts'` and `'transfers'` added to `ck_sync_deleted_kind`, copy the rows, recreate the index and the
six triggers with their latest bodies (`0016`; the recurring and transaction ones are the short forms that follow their
`_sync_unlink`).

### 4.2 `accounts`

```sql
CREATE TABLE accounts (
  id                    INTEGER PRIMARY KEY,
  sync_id               TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_accounts_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp                 TEXT,
  name                  TEXT NOT NULL CONSTRAINT ck_accounts_name CHECK (length(name) BETWEEN 1 AND 50),
  kind                  TEXT NOT NULL CONSTRAINT ck_accounts_kind CHECK (kind IN ('CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD')),
  number                TEXT CONSTRAINT ck_accounts_number CHECK (number IS NULL OR length(number) BETWEEN 1 AND 50),
  opening_balance_minor INTEGER NOT NULL DEFAULT 0
    CONSTRAINT ck_accounts_opening CHECK (opening_balance_minor BETWEEN -999999999999 AND 999999999999),
  opening_date          TEXT NOT NULL CONSTRAINT ck_accounts_opening_date
    CHECK (opening_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(opening_date) IS opening_date),
  archived              INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_accounts_archived CHECK (archived IN (0, 1)),
  position              INTEGER NOT NULL DEFAULT 0,
  created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  -- the main account is never archived, whoever writes (a sync and a restore too)
  CONSTRAINT ck_accounts_main_active CHECK (id <> 1 OR archived = 0)
) STRICT;
CREATE UNIQUE INDEX uq_accounts_sync_id ON accounts (sync_id);
```

No unique index on the name (section 16, choice 4). `position` counts from 0 over all accounts; lists sort by `position, id`.

### 4.3 The main account (the default-account trick)

Right after the table, before any trigger of `accounts` exists (so no stamp, no history row is made):

```sql
INSERT INTO accounts (id, sync_id, stamp, name, kind, opening_balance_minor, opening_date, position, created_at, updated_at)
VALUES (1, '00000000000000000000000000000001', '0000000000000-0000-00000000', 'Main account', 'CHECKING', 0, '1970-01-01', 0,
        '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z');
```

Every field is a constant, so two devices that both run the migration hold the same row, with the same `sync_id` and the oldest
possible stamp (`engine::UNSET_STAMP`): their lists already agree and nothing is sent. The first real edit on either device gets a
real stamp and wins. A new, empty database has the row too.

### 4.4 The account columns

```sql
ALTER TABLE transactions           ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1 REFERENCES accounts (id) ON DELETE NO ACTION;
ALTER TABLE recurring_transactions ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1 REFERENCES accounts (id) ON DELETE NO ACTION;
ALTER TABLE ledger_entries         ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1 REFERENCES accounts (id) ON DELETE NO ACTION;
CREATE INDEX idx_transactions_account ON transactions (account_id, payment_date);
CREATE INDEX idx_recurring_account ON recurring_transactions (account_id);
CREATE INDEX idx_ledger_account ON ledger_entries (account_id, entry_date);
```

- SQLite allows a `NOT NULL DEFAULT 1 REFERENCES` column only while foreign keys are off; `migrate::apply` runs every migration
  that way and checks `foreign_key_check` before the commit. Checked on the app's build (SQLCipher 4.19, SQLite 3.53.4), with
  foreign keys on and off around `migrate::run`: `accounts/table_tests.rs`.
- Every existing row lands in the main account with no `UPDATE`: no trigger fires, no stamp moves, both devices agree.
- The default stays: an `INSERT` that names no account (old code, the seed, a test) goes to the main account.
- Triggers made again with `account_id` among the columns they watch, bodies otherwise as the named migration left them:
  `trg_transactions_updated_at`, `trg_transactions_sync_update` (`0017`); `trg_recurring_transactions_updated_at`,
  `trg_recurring_transactions_sync_update` (`0017`); `trg_ledger_entries_updated_at` (`0012`, also `account_id` in its `WHEN`),
  `trg_ledger_entries_sync_update` (`0016`, also in its `WHEN`). Audit triggers are left alone, as in `0014` to `0017`.

### 4.5 `transfers`

```sql
CREATE TABLE transfers (
  id              INTEGER PRIMARY KEY,
  sync_id         TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_transfers_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp           TEXT,
  from_account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE NO ACTION,
  to_account_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE NO ACTION,
  amount_minor    INTEGER NOT NULL CONSTRAINT ck_transfers_amount CHECK (amount_minor BETWEEN 1 AND 999999999999),
  transfer_date   TEXT NOT NULL CONSTRAINT ck_transfers_date
    CHECK (transfer_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(transfer_date) IS transfer_date),
  note            TEXT CONSTRAINT ck_transfers_note CHECK (note IS NULL OR length(note) BETWEEN 1 AND 200),
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_transfers_two_accounts CHECK (from_account_id <> to_account_id)
) STRICT;
CREATE UNIQUE INDEX uq_transfers_sync_id ON transfers (sync_id);
CREATE INDEX idx_transfers_from ON transfers (from_account_id, transfer_date);
CREATE INDEX idx_transfers_to ON transfers (to_account_id, transfer_date);
```

### 4.6 Triggers of the two tables

Copy the triggers of `people` and `ledger_entries` (`0012`, clock as `0016`), with these columns:

| Trigger | `accounts` | `transfers` |
|---|---|---|
| `trg_<t>_updated_at` (`AFTER UPDATE OF`, stands aside while applying, only on a real change) | `id, name, kind, number, opening_balance_minor, opening_date, archived, position, created_at, updated_at` | `id, from_account_id, to_account_id, amount_minor, transfer_date, note, created_at, updated_at` |
| `trg_<t>_sync_insert` (`WHEN NEW.stamp IS NULL`) | yes | yes |
| `trg_<t>_sync_update` (new stamp only on a real change, `IS NOT` on each column) | `name, kind, number, opening_balance_minor, opening_date, archived, position` | `from_account_id, to_account_id, amount_minor, transfer_date, note` |
| `trg_<t>_sync_delete` (marker of kind `'accounts'` / `'transfers'`, stands aside while applying) | yes | yes |
| `trg_<t>_audit_insert`, `_audit_update`, `_audit_delete` (`entity_type` = the table name, every column but `sync_id`, `stamp`) | yes | yes |

One more, for the main account:

```sql
CREATE TRIGGER trg_accounts_keep_main
BEFORE DELETE ON accounts
WHEN OLD.id = 1 AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'the main account is never deleted');
END;
```

It stands aside while applying because a restore empties every table before it fills them (`backup::replace_all`); a sync
refuses a deletion of the main account itself (section 7.4). Deleting any account that still has rows is refused by the four
foreign keys (`NO ACTION`). No trigger checks "the account is active": that is a rule of the screens and the API (section 5.2),
because a sync may rightly deliver a row for an account that was archived meanwhile.

### 4.7 The view

```sql
CREATE VIEW account_movements AS
SELECT a.id AS account_id, a.opening_date AS day, a.opening_balance_minor AS delta_minor, 'OPENING' AS source, a.id AS source_id
  FROM accounts a WHERE a.opening_balance_minor <> 0
UNION ALL
SELECT t.account_id, t.payment_date, CASE c.type WHEN 'INCOME' THEN t.amount_minor ELSE -t.amount_minor END, 'TRANSACTION', t.id
  FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.paid_by IS NULL
UNION ALL
SELECT x.to_account_id, x.transfer_date, x.amount_minor, 'TRANSFER_IN', x.id FROM transfers x
UNION ALL
SELECT x.from_account_id, x.transfer_date, -x.amount_minor, 'TRANSFER_OUT', x.id FROM transfers x
UNION ALL
SELECT e.account_id, e.entry_date,
       CASE e.kind WHEN 'BORROWED' THEN e.amount_minor WHEN 'REPAID_TO_ME' THEN e.amount_minor ELSE -e.amount_minor END, 'LEDGER', e.id
  FROM ledger_entries e;
```

## 5. State machines

Refusals answer with the existing `AppError` values only: `InvalidInput` (API `400 invalid_input`), `NotFound` (`404`),
`NameExists` (`409 name_exists`). No new error code: the screens do not offer what would be refused, so a refusal is seen only
in a race or from the command line. A refused event changes nothing and makes no stamp.

### 5.1 The account

States: `NONE` (no row), `ACTIVE`, `ARCHIVED`, `DELETED` (a marker in `sync_deleted`). The main account is `ACTIVE` for ever.
"Rows" of an account: transactions, recurring items, ledger entries and transfers (either side) that point at it.

| State | Event | Guard | Next | Effect, or what a refusal answers |
|---|---|---|---|---|
| `NONE` | create | name 1 to 50 characters after trimming, no control character | `ACTIVE` | row at the end of the list (`MAX(position) + 1`); else `InvalidInput` |
| | | no `ACTIVE` account has this name, ignoring case | | else `NameExists` |
| | | kind is one of the four; number absent or 1 to 50 clean characters; opening balance within ±9999999999.99; opening date a real date | | else `InvalidInput` |
| `ACTIVE` | update (any of name, kind, number, opening balance, opening date) | the same checks; the name check skips the account itself | `ACTIVE` | fields written, new stamp only if something changed |
| `ACTIVE` | reorder | the ids are exactly the `ACTIVE` accounts, each once | `ACTIVE` | only rows whose position changes are written; else `InvalidInput` |
| `ACTIVE` | archive | not the main account | `ARCHIVED` | `archived = 1`; rows untouched |
| `ACTIVE`, main | archive | | `ACTIVE` | refused: `InvalidInput` |
| `ACTIVE` | restore | | `ACTIVE` | answers the account, nothing written (as `people::restore`) |
| `ACTIVE` | delete | not the main account and no rows | `DELETED` | row removed, marker made |
| `ACTIVE`, main | delete | | `ACTIVE` | refused: `InvalidInput` (the trigger is the net under it) |
| `ACTIVE` | delete | has rows | `ACTIVE` | refused: `InvalidInput` |
| `ARCHIVED` | archive (again) | | `ARCHIVED` | answers the account, nothing written |
| `ARCHIVED` | restore | no `ACTIVE` account has its name | `ACTIVE` | `archived = 0`; else `NameExists` |
| `ARCHIVED` | update, reorder | | `ARCHIVED` | refused: `InvalidInput` (restore it first) |
| `ARCHIVED` | delete | no rows | `DELETED` | row removed, marker made; with rows `InvalidInput` |
| `ARCHIVED` | new row, or a row moved into it | | `ARCHIVED` | refused: `InvalidInput` (section 5.2) |
| any | currency conversion | | same | opening balance converted (section 8) |
| `NONE`, `DELETED` | any event by id | | same | `NotFound` |
| `NONE` | sync: row arrives | row valid (section 7.2) | `ACTIVE` or `ARCHIVED` | inserted as sent |
| `ACTIVE`, `ARCHIVED` | sync: newer row arrives | | as the row says | whole row replaced |
| `ACTIVE`, `ARCHIVED` | sync: older row arrives | | same | kept yours |
| `ACTIVE`, `ARCHIVED` | sync: newer marker arrives | not the main account | `DELETED` | its rows here move to the main account first (section 7.4) |
| `ACTIVE`, `ARCHIVED` | sync: older marker arrives | | same | kept yours; the other device gets the row back |
| `DELETED` | sync: row newer than the marker | | `ACTIVE` or `ARCHIVED` | marker removed, row inserted |
| `DELETED` | sync: row older than the marker | | `DELETED` | dropped |
| main | sync: marker for it, or a row with `archived = 1` | | | the sync stops: `Invalid("account")` |

Why the main account can be neither archived nor deleted (and not "the last active account"): the rule holds on each device
alone. With "the last active account", two devices could each archive a different one of the last two and end with none.
With this rule there is always an active account, so "archive the last active account" is the same refusal.

### 5.2 A row and its account

For a transaction, a recurring item, a ledger entry, and each side of a transfer. States: `IN_ACTIVE`, `IN_ARCHIVED`.

| State | Event | Guard | Next | Effect, or what a refusal answers |
|---|---|---|---|---|
| none | create, no account named | | `IN_ACTIVE` | the main account |
| none | create in A | A exists and is `ACTIVE` | `IN_ACTIVE` | else `InvalidInput` (missing and archived alike, as an unknown category) |
| `IN_ACTIVE` | move to B | B differs, exists and is `ACTIVE` | `IN_ACTIVE` | `account_id = B`, new stamp; else `InvalidInput` |
| either | update that names the account it already has | | same | not a move: allowed, no account check |
| `IN_ACTIVE` | its account is archived | | `IN_ARCHIVED` | the row is not written: no stamp |
| `IN_ARCHIVED` | edit other fields, delete | | `IN_ARCHIVED` / gone | allowed, as a transaction keeps a deleted category |
| `IN_ARCHIVED` | move to an `ACTIVE` B | | `IN_ACTIVE` | allowed |
| `IN_ARCHIVED` | its account is restored | | `IN_ACTIVE` | nothing written to the row |
| recurring item, `IN_ARCHIVED` | pay | | item stays | the payment is made in the **main account**; the item is not changed |
| recurring item, `IN_ACTIVE` | pay | | | the payment copies the item's `account_id` |
| either | its account is deleted here | | | cannot happen: delete is refused while rows exist |
| none | sync: row arrives, its account is here | | either | placed in that account, archived or not |
| none | sync: row arrives, its account is not here and has no marker | | | the sync stops: `Invalid("reference")`. Accounts are applied before every table that points at them (section 7.3), so an honest peer never causes it, whatever order the messages came in |
| none | sync: row arrives, its account has a marker here | | `IN_ACTIVE` | placed in the main account, stamp = the later of the row's stamp and the marker's |
| either | sync: a marker for its account arrives | | `IN_ACTIVE` | moved to the main account, same stamp rule (section 7.4) |

A split bill a friend paid still has an account (the column is never empty), but it moves no balance (section 3).

### 5.3 A transfer

States: `NONE`, `LIVE`, `DELETED` (marker).

| State | Event | Guard | Next | Effect, or what a refusal answers |
|---|---|---|---|---|
| `NONE` | create | both accounts exist and are `ACTIVE` | `LIVE` | else `InvalidInput` |
| | | from ≠ to | | else `InvalidInput` (the `CHECK` is the net) |
| | | amount "0.01" to "9999999999.99" (0, negative, three decimals refused); a real date; note at most 200 characters, line breaks allowed, empty means none | | else `InvalidInput` |
| `LIVE` | update | the same checks; a side that does not change may be archived; a side that changes must be `ACTIVE` | `LIVE` | fields written, new stamp only on a change |
| `LIVE` | delete | | `DELETED` | row removed, marker made |
| `LIVE` | a side is archived or restored | | `LIVE` | nothing written; it still counts in both balances |
| `LIVE` | currency conversion | | `LIVE` | amount converted, at least one cent |
| `NONE`, `DELETED` | update, delete | | same | `NotFound` |
| any | sync: row with from = to, amount < 1, a bad date or note | | | the sync stops: `Invalid("row")` |
| `NONE` | sync: row arrives | both accounts here | `LIVE` | inserted (sides may be archived) |
| `NONE` | sync: row arrives, a side unknown with no marker | | | the sync stops: `Invalid("reference")` |
| `NONE`, `LIVE` | sync: a side's account was deleted (marker here, or arriving) | the other side is not the main account | `LIVE` | that side becomes the main account, stamp = the later of the two |
| `NONE`, `LIVE` | the same | both sides would be the main account | `DELETED` | the transfer is dropped: marker with stamp `after(the later of the two)` on both devices |
| `LIVE` | sync: newer row / older row / newer marker / older marker | | | the ordinary rules: newer wins the whole row |

### 5.4 The dialogs

The add and edit account dialog (`AccountModal`) and the transfer dialog (`TransferModal`) follow the pattern of the existing
dialogs (`Modal` with `pending`, toasts through `useToast`, `errorText`). There is no error state on screen: "invalid" and
"failed" are a toast and a return to `EDITING`.

| State | Event | Guard | Next | Effect |
|---|---|---|---|---|
| `CLOSED` | open (add) | | `EDITING` | empty fields with the defaults of section 13 |
| `CLOSED` | open (edit) | | `EDITING` | fields hold the row |
| `EDITING` | submit | a field is wrong | `EDITING` | error toast (`accounts.nameError`, `accounts.balanceError`, `tx.errorAmount`, `transfers.errorAccounts`); nothing sent; fields kept |
| `EDITING` | submit | all fields right | `SAVING` | request sent; **done** is disabled |
| `EDITING` | cancel, `Esc`, click outside | | `CLOSED` | nothing saved |
| `SAVING` | submit again | | `SAVING` | ignored (`if (pending) return`) |
| `SAVING` | answer ok | | `CLOSED` | the list reloads; the last used account is remembered (transfer: the from account) |
| `SAVING` | answer error | | `EDITING` | error toast `errorText(error)` (`name_exists`: "that name already exists."); fields kept |
| `SAVING` | cancel | | `CLOSED` | the request still finishes; the page reloads the list when it does (the page holds the handler) |

An edit sends only the fields that changed (a patch). A save with nothing changed closes the dialog and sends nothing.

### 5.5 Weird transitions the tests must cover

1. Archive twice: the second answers the account, makes no stamp.
2. Restore an account that was never archived: answers the account, makes no stamp.
3. Delete the main account; archive the main account; a row or marker from a sync that does either.
4. Archive the last active account besides the main one, then try the main one: refused, one active account stays.
5. Delete an account with one transaction; with only a recurring item; with only a ledger entry; with only a transfer on its
   `to` side: each refused. Delete after the last row is moved away: allowed.
6. Create or move a transaction, recurring item, ledger entry or transfer side into an archived account: refused. Edit the
   amount of a transaction already in an archived account: allowed. Update it naming the same archived account: allowed.
7. A transfer to itself; amount "0.00"; amount "-1.00"; an unknown account; edit a transfer so both sides are equal.
8. Pay a recurring item whose account is archived: the payment is in the main account.
9. Sync delivers a transaction with no row for its account (refused), and the same batch with the account row in any message
   order (applied).
10. Device A deletes an empty account while device B adds a transaction, a recurring item, a ledger entry and a transfer into it:
    after the sync both hold the rows in the main account with equal stamps; the transfer between that account and the main
    account is gone on both, with equal markers.
11. A edits an account after B deleted it (newer row): the account is back on both. A archives while B renames: newer whole row wins.
12. Both devices rename different accounts to the same name: both keep both, no merge, no error. Local create or rename to a
    taken name: `NameExists`. Restore an archived account whose name was taken meanwhile: `NameExists`.
13. Both devices run the migration, then sync: one main account, nothing sent, no conflict. One device renames it first: the
    rename arrives. Both rename it: an ordinary conflict.
14. A fresh phone (only the untouched main account) is empty for pairing; a phone whose main account was edited is not.
15. Currency conversion with an archived account, a negative opening balance, an opening balance of 0, a transfer of one cent at
    a rate of 0.001 (stays one cent), an opening balance that leaves the range (the whole change is refused), and the same
    conversion arriving by sync: both devices equal.
16. Opening balance "-9999999999.99" and "9999999999.99" accepted; one cent beyond, "-0.00", "1.5", "+1.00" refused.
17. Opening date after existing rows, before them, and in the future: the balance on each side of the date.
18. A future-dated transaction and transfer: not in today's balance, in the balance of their day.
19. A split bill you paid (whole bill), one a friend paid (nothing), "my share of a bill" (expense and entry cancel), a
    repayment in another account than the bill.
20. Reorder with a missing id, a repeated id, an archived id, the id of another list: refused.
21. Restore a backup made before `0018`: one main account holding everything. Restore a `0018` backup without a main account,
    or with it archived: refused whole.

## 6. Rust core: what package 1 exposes

All in `src-tauri/src/accounts.rs` (accounts, balances, statement, summary) and `src-tauri/src/accounts/transfers.rs`.
Ids cross as strings (`serialize_id`, `parse_id`); money as decimal strings; JSON is camelCase with `deny_unknown_fields` on inputs.

### 6.1 Helpers

- `money::parse_signed_amount(text) -> Result<i64, AppError>`: `parse_amount` with an optional leading `-`; `-0.00` refused.
- `accounts::is_active(conn, id) -> Result<bool, AppError>` and `accounts::for_new_row(conn, account_id: Option<&str>) ->
  Result<i64, AppError>` (none: `MAIN_ID`; given: parsed, must be active, else `InvalidInput`). Every create uses it. Every
  update uses `accounts::for_moved_row(conn, current: i64, account_id: Option<&str>)` (crate only): none, or the account the row
  already has (archived or not): `current`; another one: as `for_new_row`.
- `accounts::check(conn) -> Result<bool, AppError>`: the main account exists with `id` 1, `MAIN_SYNC_ID` and `archived = 0`.
  Called by a sync before its commit and by a restore (as `splits::check`).

### 6.2 Accounts

```
Account { id, name, kind, number: string|null, openingBalance: "-120.00", openingDate: "2026-01-01", archived: bool,
          isMain: bool, balance: "1520.35", changeThisMonth: "-80.10", lastActivity: "2026-11-02"|null, entries: 42 }
NewAccount   { name, kind, number?, openingBalance? ("0.00"), openingDate? (today) }
AccountPatch { name?, kind?, number? (null clears), openingBalance?, openingDate? }
Summary      { inAccounts: "5210.00", friends: "50.00", netWorth: "5260.00", accounts: 3, inUse: bool }
Statement    { account: Account, from, to, balanceBefore: "900.00", lines: [Line], balanceAfter: "1520.35" }
Line         { date, source: "OPENING"|"TRANSACTION"|"TRANSFER_IN"|"TRANSFER_OUT"|"LEDGER", id: string, what: string,
               other: string|null, amount: "-12.50" (signed, what it did to the account), balance: "887.50" }
HistoryPoint { month: "2026-10", balance: "1520.35" }
```

`entries` is the number of rows (section 5.1); the screens offer delete when `!isMain && entries == 0`. `inUse` is true when
there is more than one account or the main account's opening balance is not 0. `Line.what` is the transaction's name, the
transfer's note or the entry's note (empty when none); `Line.other` is the category name, the other account's name, or the
friend's name. Lines are oldest first, ordered by `(date, source order as listed, id)`, with `balance` running from
`balanceBefore`, as `ledger::statement` does.

| Function | Tauri command | Answer |
|---|---|---|
| `list(conn, today) -> Vec<Account>`: active by `position, id`, then archived by `position, id` | `list_accounts()` | `Account[]` |
| `get(conn, id, today)` | (used by the others) | `Account` |
| `create(conn, &NewAccount, today)` | `create_account(input)` | `Account` |
| `update(conn, id, &AccountPatch, today)` | `update_account(id, patch)` | `Account` |
| `archive(conn, id, today)` | `archive_account(id)` | `Account` |
| `restore(conn, id, today)` | `unarchive_account(id)` | `Account` |
| `delete(conn, id)` | `delete_account(id)` | `null` |
| `reorder(conn, &[i64])` | `reorder_accounts(ids)` | `null` |
| `statement(conn, id, from, to, today)`: `from` and `to` inclusive days | `account_statement(id, from, to)` | `Statement` |
| `history(conn, id, year, today)` | `account_history(id, year)` | `HistoryPoint[]` (up to 12) |
| `summary(conn, today)` | `accounts_summary()` | `Summary` |

`today` comes from `today_text(&vault)` in `commands.rs`, as for recurring. `year` is text ("2026"), as `stats::year_trend`
takes it. `reorder` takes `&Connection` (its own transaction inside), not `&mut` as `categories::reorder`.

### 6.3 Transfers

```
Transfer      { id, fromAccountId, toAccountId, amount: "200.00", date: "2026-11-03", note: string|null }
NewTransfer   { fromAccountId, toAccountId, amount, date, note? }
TransferPatch { fromAccountId?, toAccountId?, amount?, date?, note? (null clears) }
Filter        { accountId?, from?, to?, limit?, offset? }   Page { transfers: [Transfer], total, limit, offset }
```

| Function | Tauri command |
|---|---|
| `transfers::create(conn, &NewTransfer) -> Transfer` | `create_transfer(input)` |
| `transfers::get(conn, id)` | `get_transfer(id)` (the statement row opens the edit dialog with it) |
| `transfers::update(conn, id, &TransferPatch)` | `update_transfer(id, patch)` |
| `transfers::delete(conn, id)` | `delete_transfer(id)` |
| `transfers::list(conn, &Filter) -> Page`, newest first, limits as `transactions::list` | none (the API route uses it) |

### 6.4 Existing types that gain the account

| Where | Change |
|---|---|
| `transactions.rs` | `Transaction.account_id` (JSON `accountId`); `NewTransaction.account_id: Option<String>`; `TransactionPatch.account_id: Option<String>`; `Filter.account_id: Option<String>`; `COLUMNS`, `create`, `update` (section 5.2), `list` |
| `recurring.rs` | `Recurring.account_id`; `NewRecurring.account_id`, `RecurringPatch.account_id` (both optional); `pay` writes the item's account, or `MAIN_ID` when that account is archived |
| `debts/ledger.rs` | `Entry.account_id`; `NewEntry.account_id`, `EntryPatch.account_id` (optional); `borrow_in` puts the expense and the entry in the same account; `settle` gets `account_id: Option<&str>` as its last argument before `today` |
| `debts/codes.rs` | `Accept.account_id: Option<String>`, passed to the expense and the entry |
| `commands.rs` | the commands above; `settle_person` gets `account_id: Option<String>`. Each new command is added to `COMMANDS` in `commands.rs`, to `COMMANDS` in `src-tauri/build.rs`, to `generate_handler!` in `src-tauri/src/lib.rs`, and as `allow-<name>` to `src-tauri/capabilities/default.json` |

All new input fields are optional, so the screens and the command line of today keep working before package 5 lands.

## 7. Sync

Protocol **9**. Two devices with different protocol or schema versions refuse to sync and are told to update, as now.

### 7.1 Tables and order

`wire::Table` gains `Accounts` and `Transfers`. `Table::ALL`, sending and applying order: settings, categories, people,
**accounts**, recurring, budgets, transactions, **transfers**, ledger, documents, links. `engine::LISTED` and
`stamp::STAMPED_TABLES` gain both. `engine::is_empty` also needs `transfers` empty and no account other than the untouched
main one: `SELECT count(*) FROM accounts WHERE sync_id <> MAIN_SYNC_ID OR stamp <> UNSET_STAMP` is 0.

### 7.2 Wire rows

```
AccountRow  { syncId, stamp, name, kind, number, openingBalanceMinor, openingDate, archived, position, createdAt, updatedAt }
TransferRow { syncId, stamp, from, to, amountMinor, transferDate, note, createdAt, updatedAt }     // from, to: account sync ids
TransactionRow, RecurringRow, LedgerRow: one new field  account: String                          // the account's sync id
```

Checks in `parse_row`, nothing trimmed or repaired: name as a category name; kind one of the four; number absent or 1 to 50
characters with no control character and no outer spaces; `openingBalanceMinor` within ±`MAX_MINOR`; a real date; `archived` 0 or
1; `position` 0 to `MAX_POSITION`; the main account's row has `archived` 0. Transfer: two valid ids that differ, `money_ok(amount, 1)`,
a real date, `note_ok`. `account` on the three old rows is a valid sync id.

### 7.3 Applying

- `Applier::accounts`: by `sync_id`, the newer stamp wins the whole row; a marker newer than the row keeps it deleted, an older
  one is removed. **Accounts are never merged by name**: two accounts with one name can be two real accounts, and a merge would
  add their balances.
- `Applier::account(sync_id) -> (local id, deleted_at)`: the account if it is here; `MAIN_ID` and the stamp of its marker when it has a
  marker here; otherwise `Invalid("reference")`. Used by `recurring`, `transactions`, `ledger` and the new `transfers`. A moved row takes
  `max(its stamp, the marker's stamp)`; `updated_at` stays as sent.
- `Applier::transfers`: the ordinary rules. After resolving both sides, a transfer whose sides are equal is not written: an
  existing row is deleted and a marker with stamp `after(that max)` is put.
- Before the commit, with the other checks: `accounts::check` (else `Invalid("account")`), then `foreign_key_check`.

### 7.4 Deletions

Markers are applied in this order: ledger, transactions, transfers, budgets, recurring, links, documents, **accounts last**, so a
row deleted in the same batch is gone before an account's deletion would stamp it. For an account marker newer than the local
row: a marker for `MAIN_SYNC_ID` stops the sync (`Invalid("account")`); otherwise, with `M` the marker's stamp,

```sql
UPDATE transactions           SET account_id = 1, stamp = max(stamp, M) WHERE account_id = X;   -- same for
UPDATE recurring_transactions ...; UPDATE ledger_entries ...;                                   -- these two
UPDATE transfers SET from_account_id = 1, stamp = max(stamp, M) WHERE from_account_id = X AND to_account_id <> 1;
UPDATE transfers SET to_account_id = 1,   stamp = max(stamp, M) WHERE to_account_id = X AND from_account_id <> 1;
-- what is left points at X on one side and at the main account on the other: delete each, marker after(max(stamp, M))
DELETE FROM accounts WHERE id = X;   -- then put the marker
```

The device that deleted the account does the same from the other end (section 7.3), so both finish equal. Stamps here come
from the stamps involved, never the clock (`sync-design.md`, section 11).

### 7.5 Conflicts

Accounts and transfers are ordinary rows in the conflict review. `engine::facts` gains: for an account `name`, `type`, `number`,
`openingBalance`, `openingDate`, `archived`; for a transfer `from`, `to`, `amount`, `date`, `note`; and `account` (the name on
this device) for transactions, recurring items and ledger entries. Their words are `sync.field.*` texts (section 14).

### 7.6 What goes into `docs/sync-design.md`

- Section 8, hello: "protocol 9: accounts and transfers".
- Section 9: add `0018` to the migrations; `accounts` and `transfers` to the shared tables; and the sentence: "`accounts` (the
  main account has the same `sync_id` and the oldest stamp on every device, so it is one account without a merge;
  `accounts.position` travels in the row) and `transfers`; `account_id` of `transactions`, `recurring_transactions` and
  `ledger_entries`, sent as the account's `sync_id`; part of protocol 9".
- Section 11: "Accounts are never merged by name. Rows of an account deleted on the other device move to the main account and
  take the later of their stamp and the deletion's; a transfer left with the main account on both sides becomes a marker.
  `accounts-design.md`, section 7."
- Section 12: the new fixed order (7.1).

## 8. Currency change

`src-tauri/src/currency.rs`, in `change` and `apply_recorded`, next to the other four conversions:

- `convert_accounts`: every `opening_balance_minor` that is not 0. `convert` assumes amounts are not negative, so a negative one
  is converted as its absolute value and the sign put back (half away from zero); the result may be 0 and must stay within
  ±`MAX_MINOR`, else `InvalidInput` and nothing changes.
- `convert_transfers`: every `amount_minor`, `at_least_one(convert(...))`, as ledger entries.
- `Conversion` gains `accounts: u32` and `transfers: u32` (rows converted).
- Archived accounts convert like the others. Triggers stand aside (`applying`), so no stamp moves, and the phone converts its
  own rows the same way: both devices get the same numbers. A balance is a sum of converted pieces, so it can differ from the
  converted old balance by a few cents, as a friend's balance does (`debts-design.md`, section 8).

## 9. Backup, restore, "Delete all my data"

- `src-tauri/src/backup.rs`: `TABLES` gains `"accounts"` after `"people"` and `"transfers"` after `"accounts"` (parents before
  children; the existing test fails until they are there).
- `replace_all` calls `accounts::check` when the schema has the table, next to `splits::check`; a file without a valid main
  account is refused whole (`InvalidBackup`), and so is a file with a row that points at an account it does not hold, or with the
  main account archived (the foreign keys and `ck_accounts_main_active` refuse those at the row). All of it happens in the one
  transaction, so a refusal leaves the database as it was.
- A backup from before `0018` is loaded at its own version and migrated (`prepare`): the migration makes the main account and
  puts everything in it.
- "Delete all my data" removes the database; the next one has the main account from the migration.

## 10. Exports and the report

- **JSON** (`src-tauri/src/export.rs`): `Section` gains `Accounts` and `Transfers`. Each transaction, recurring item and ledger
  entry gains `accountId` and `account` (the name). With no limit: `accounts` (every field of `Account` but `entries`; `balance`
  is the balance on the day of the export) and `transfers` (`id`, `date`, `fromAccountId`, `from`, `toAccountId`, `to`, `amount`,
  `note`). With a limit (dates, categories, ...): `accounts` holds only the accounts that the rows of the file are in (its
  transactions, recurring items and ledger entries, of the parts that are asked for), as `id`, `name`, `kind` (a balance or a
  number would tell about the rest), and there is no `transfers` key at all (a file does not say what it was told to leave out;
  `Parsed::with_transfers`). `Preview` gains the two counts, `accounts` and `transfers`, 0 for a part that is left out.
  The account of a row is read by the id of the row (`account_of_rows`), so `export/debts.rs` does not change.
- **CSV**: one more column at the end, `account`, so the columns of today keep their place.
- **Excel** (`src-tauri/src/export/xlsx.rs`): an `Account` column at the end of the transactions, recurring and ledger sheets;
  sheets `Accounts` (Name, Type, Number, Opening balance, Opening date, Archived, Balance) and `Transfers` (Date, From, To,
  Amount, Note), under the same limit rule: with a limit the `Accounts` sheet has only the Name and Type cells filled and there
  is no `Transfers` sheet (so a workbook of the transfers alone is refused under a limit, as one of the change history is).
- **PDF report** (`src-tauri/src/report.rs`, `report/layout.rs`): in a report of every category, when `Summary.inUse`, a short
  table "Accounts" (after the budgets, before the list of transactions; in a month with no transactions it takes the place of the
  "no transactions" line): per account, the archived ones last, the balance on the last day of the period and the change over
  the period, and a last line "net worth": what every account held that day plus what friends owed you that day, as the dashboard
  counts it. Whether accounts are in use is asked before the report's read transaction (`accounts::summary` has its own).
  Four new `ReportLabels` (`accounts`, `balance`, `change`, `net_worth`), with their English text and the UI's
  `ui/src/api/reportLabels.ts`. The four are optional when a report is asked for (`serde(default)`, in English), because the
  command line (`api/confirmed.rs`) and older tests build their labels without them; the app always sends all of them.
- Statements of debts (`export/debts.rs`) do not change.

## 11. Demo data

`src-tauri/seeds/dev.sql`, with `src-tauri/src/seed.rs` tests: the main account becomes "Current account" (an IBAN, an opening
balance sixteen months back); "Savings" (`SAVINGS`), "Cash" (`CASH`), "Credit card" (`CREDIT_CARD`, a negative balance) and one
archived account ("Old savings") with history; some transactions in cash and on the card; a recurring item on the card (the
streaming plan, with its payments); a monthly transfer to savings and one that pays off the card (each month what was spent on
it the month before, so it stays in the red); cash from the bank every third month; one repayment from a friend in cash. The seed test counts 5 accounts and at least 16
transfers and checks `accounts::check`. `examples/dev_vault.rs` and `examples/api_demo.rs` need no change beyond that.

## 12. Command line and API

Routes in a new `src-tauri/src/api/accounts.rs` (tests `api/accounts_tests.rs`), listed in `ALL` in `api/routes.rs`.

| Route | Scope | Arguments | Command |
|---|---|---|---|
| `accounts.list` | read | | `meowny accounts list` |
| `accounts.show` | read | `id`, `from`, `to` (default: this month) | `meowny accounts show 2 --from 2026-11-01 --to 2026-11-30` |
| `accounts.history` | read | `id`, `year` | `meowny accounts history 2 --year 2026` |
| `accounts.summary` | read | | `meowny accounts summary` |
| `transfers.list` | read | `accountId`, `from`, `to`, `limit`, `offset` | `meowny transfers list` |
| `accounts.create` | write | `name`, `kind`, `number`, `openingBalance`, `openingDate` | `meowny accounts create --name Savings --kind savings` |
| `accounts.update` | write | `id` and any of those | `meowny accounts update 2 --name "Rainy day"` |
| `accounts.archive`, `accounts.unarchive` | write | `id` | `meowny accounts archive 2` |
| `accounts.delete` | write | `id` (asks `--yes`, by its verb) | `meowny accounts delete 2 --yes` |
| `accounts.reorder` | write | `ids` | `meowny accounts reorder --ids 1,3,2` |
| `transfers.create` | write | `fromAccountId`, `toAccountId`, `amount`, `date`, `note` | `meowny transfers create --from-account-id 1 --to-account-id 2 --amount 200.00 --date 2026-11-03` |
| `transfers.update` | write | `id` and any of those | |
| `transfers.delete` | write | `id` (asks `--yes`) | |

- New optional argument `accountId` (`Kind::Id`) on `transactions.create`, `transactions.update`, `transactions.list`,
  `recurring.create`, `recurring.update`, `ledger.add`, `ledger.update`, `people.settle`.
- `kind` is a `Kind::String` that lists its four words in capitals, accepted in any case on the command line (`savings`, `Credit_Card`), as `categories.create --type`: the command line matches the words without regard to case; the HTTP API takes them exactly.
- `openingBalance` is a `Kind::String`, not `Kind::Money` (which has no sign); `parse_signed_amount` checks it. On the command
  line a negative value is written `--opening-balance=-120.00`; `--opening-balance -120.00` works too, because the word after a flag is always
  its value (`cli/src/args.rs`).
- After the routes: `cd cli && cargo run --example dump_schema > schema.json`; examples in the tour of `cli/tests/docs.rs`
  (list, create, update, show, a transfer, summary, archive, a transaction with `--account-id`, and one refusal: archiving the main
  account); `cd cli && UPDATE_DOCS=1 cargo test --offline --test docs`; `npm run docs`. The `meowny` skill is out of scope.

## 13. The screens

One page serves the computer and the phone. Reuse, do not rebuild: `Table`, `Row`, `TapRow`, `IconAction` (`RowParts.tsx`),
`useOpenRow`, `useReorder`, `Modal`, `ConfirmModal`, `Dropdown`, `Pick`, `DatePicker`, `MonthNav`, `EmptyState`, `Card`,
`formatMoney`, `saveChoice`/`storedId`/`storedChoice` (`lib/sticky.ts`), `useToast`, `TransactionLink`.

**The rule that hides it all:** `several` = more than one account that is not archived. When it is false, no form shows an
account field, the transactions page has no account filter or column, and nothing asks about accounts.

**Which account a new row starts with** (`lib/accountsView.ts`, `defaultAccountId`): the last one used (`choice.account.last`,
an id) if it is still active, else the first active account in the list. It is remembered when a row is saved.

- **Accounts page** (`pages/AccountsPage.tsx`, route `accounts/:accountId?`, in `pages/index.ts`, `App.tsx`, `AppSidebar.tsx`
  after transactions with icon `accounts` in `NavIcon.tsx`, shortcut `goAccounts` = `g` then `a` in `keys/shortcuts.ts`; on the
  phone an icon in the top bar before documents).
  - Toolbar: **add account**; **transfer** (shown with two or more active accounts); the total "in accounts" at the right.
  - Table, in the person's order, reordered as categories are (hold and drag, `Alt` + arrows): name, type, number, last
    activity, this month (signed, green or red), balance. Row buttons: edit; archive (not on the main account); delete in its
    place when `entries == 0`. A click on the row opens the account. Phone: `TapRow` with the balance as the amount and
    "type · last activity · this month" under the name; the panel has **open**, **edit**, **transfer**, **archive** or **delete**.
  - Archived accounts apart: a switch **archived (n)** under the list (remembered, `choice.accounts.archived`) opens a second
    list: name, type, balance, **restore**, and **delete** when empty.
  - Archive asks first (`ConfirmModal`); when the balance is not 0 the question says how much the account still holds.
    Delete asks first.
- **An account** (same page, the account in the address, as the debts page shows a friend; **back** returns to the list):
  name, type, number and the balance; `BalanceChart` (`components/BalanceChart.tsx`: one SVG line, the hand-drawn kind of
  `YearCharts` in `DashboardPage.tsx`, with the year arrows and hover of that chart); a month to step through (`MonthNav`); the
  statement of that month as a table: date, what, category / other account / friend, amount (signed), balance after. A
  transaction row links to it on the transactions page (`TransactionLink`); a transfer row opens `TransferModal` to edit or
  delete it; a ledger row links to the friend. Buttons: **transfer** (from this account), **edit**.
- **`AccountModal`** (`components/AccountModal.tsx`): name; type (`Dropdown`, starts at checking); number (optional); opening
  balance (optional, may start with a minus, empty means 0.00); "on" date (`DatePicker`). The date starts at today for a new
  account and when the stored opening balance is 0.00, else at the stored date. Section 5.4.
- **`TransferModal`** (`components/TransferModal.tsx`): from, to (`Dropdown`s of the active accounts), amount, date (today), note
  (optional). Defaults: from = the account of the page, else `defaultAccountId`; to = the first other active account. Choosing in
  one list the account the other holds swaps the two, so the same account on both sides cannot be made. Editing a transfer with
  an archived side shows that account with "(archived)" and keeps it unless changed.
- **Transaction form** (`TransactionModal` in `pages/TransactionsPage.tsx`, and `components/QuickAdd.tsx` on the phone): an
  **account** `Dropdown` after the category when `several`. A row whose account is archived shows it as "name (archived)".
  Without the field, a new row is sent with no `accountId`.
- **Transactions page**: when `several`, an account `Pick` beside the category one (remembered as `choice.tx.account`, dropped
  when that account is gone, behind **filters** on the phone and counted there), and the account's name in the row.
- **Recurring form** (`pages/RecurringPage.tsx`): the same `Dropdown`. A paid occurrence takes it (section 5.2).
- **Debts dialogs** (`EntryModal.tsx`, `SettleModal.tsx`, `BillAcceptModal.tsx`): the same `Dropdown` when `several`, labelled
  **account**, so a repayment in cash goes to the cash account.
- **Dashboard** (`pages/DashboardPage.tsx`): when `Summary.inUse`, a card **net worth** in the grid: the number; under it "in
  accounts" and, when it is not 0, "friends"; the card links to the accounts page. Otherwise nothing new is shown.
- **Sync review** (`components/ReviewModal.tsx`): the new field words. **Export page**: the two new sections in the list of parts.
- **API modules**: `ui/src/api/accounts.ts`, `ui/src/api/transfers.ts` (Zod schemas of section 6; signed money is
  `/^-?\d+\.\d{2}$/`), and the new fields in `api/transactions.ts`, `api/recurring.ts`, `api/debts.ts`, `api/dataExport.ts`.
  `lib/money.ts` gains `signedAmountString(raw)` (as `amountString`, with an optional minus, zero allowed).
- **The main account's name**: `accountName(account, t)` in `lib/accountsView.ts` shows `accounts.mainName` in the person's
  language while the stored name is still exactly `Main account`; any rename shows as typed.

## 14. Texts

New keys for `ui/src/i18n/en.ts` (lower case, as the others). The six other files get the same keys, translated.

```
'nav.accounts': 'accounts',
'accounts.title': 'accounts',
'accounts.add': 'add account',
'accounts.addTitle': 'new account',
'accounts.editTitle': 'edit account',
'accounts.mainName': 'main account',
'accounts.mainHint': 'the main account always stays. new entries go here when no account is chosen.',
'accounts.account': 'account',
'accounts.allAccounts': 'all accounts',
'accounts.kind.CHECKING': 'checking',
'accounts.kind.SAVINGS': 'savings',
'accounts.kind.CASH': 'cash',
'accounts.kind.CREDIT_CARD': 'credit card',
'accounts.number': 'account number',
'accounts.numberHint': 'an IBAN or anything that helps you tell it apart',
'accounts.openingBalance': 'opening balance',
'accounts.openingDate': 'on',
'accounts.balance': 'balance',
'accounts.lastActivity': 'last activity',
'accounts.noActivity': 'nothing yet',
'accounts.thisMonth': 'this month',
'accounts.inAccounts': 'in accounts',
'accounts.nameError': 'enter an account name of up to 50 characters.',
'accounts.balanceError': 'enter a balance with at most two decimals. it can start with a minus.',
'accounts.archive': 'archive',
'accounts.restore': 'restore',
'accounts.archived': 'archived',
'accounts.archivedCount': 'archived ({count})',
'accounts.archivedSuffix': '{name} (archived)',
'accounts.archiveText': 'it leaves the lists. its history and balance stay, and you can restore it.',
'accounts.archiveHolds': 'it still holds {amount}. move it out with a transfer first if the account is closed.',
'accounts.deleteText': 'this account has nothing in it and will be deleted for good.',
'accounts.open': 'open',
'accounts.empty': 'nothing moved in this month.',
'accounts.reorderHint': 'hold and drag to reorder, or use alt and the arrow keys',
'accounts.reordered': '{name} moved to position {position} of {count}',
'accounts.chart': 'balance over the year, by month',
'accounts.balanceAfter': 'balance after',
'accounts.opening': 'opening balance',
'accounts.netWorth': 'net worth',
'accounts.friends': 'friends',
'transfers.add': 'transfer',
'transfers.addTitle': 'new transfer',
'transfers.editTitle': 'edit transfer',
'transfers.from': 'from',
'transfers.to': 'to',
'transfers.in': 'transfer from {name}',
'transfers.out': 'transfer to {name}',
'transfers.errorAccounts': 'choose two different accounts.',
'sync.field.account': 'account',
'sync.field.number': 'account number',
'sync.field.openingBalance': 'opening balance',
'sync.field.openingDate': 'opening date',
'sync.field.archived': 'archived',
'sync.field.from': 'from',
'sync.field.to': 'to',
'sync.field.note': 'note',
'export.partAccounts': 'accounts',
'export.partTransfers': 'transfers',
'report.accounts': 'accounts',
'report.balance': 'balance',
'report.change': 'change',
'report.netWorth': 'net worth',
```

Reused as they are: `common.name`, `common.type`, `common.optional`, `common.delete`, `common.edit`, `common.back`, `tx.amount`,
`tx.errorAmount`, `report.date`, `debts.note`, `rec.confirmTitle`, `keys.goTo`, `tx.filters`. Before adding a `sync.field.*`,
`export.*` or `report.*` key, check that the file does not already have it under that name.

## 15. Guide

`docs/site/guide.mjs`:

- **Budgeting**: the opening sentence names the accounts page; **Dashboard** gets the net worth line; **Transactions** gets one
  step ("with more than one account, choose the account"); **Find transactions** gets the account filter; **Recurring items**
  one sentence. Two new parts after **Find transactions**: `accounts` ("Accounts": what an account is, add, edit, reorder,
  archive and restore, delete an empty one, the main account, what the balance counts, split bills in one sentence) and
  `transfers` ("Move money between accounts").
- **Debts**: in "Friends and the history" and "Mark as paid", one sentence: with several accounts, choose where the money went.
- **Your phone**, "On the phone": accounts is an icon in the top bar. **Backup**, "Export": accounts and transfers are in the files.
- **Keyboard shortcuts**: `g` then `a`.
- **Privacy**, "What is stored": the first row becomes "Categories, budgets, accounts (names, numbers, opening balances),
  transfers, transactions, ..."; the filters row gains "the account filter, whether archived accounts are shown, the last account
  used (a number)". "What leaves the device" does not change: sync, backups and exports carry the accounts like the rest.
- `docs/site/shots.mjs` and `docs/site/images/`: `accounts`, `account` (and `phone-accounts`), light and dark.
- `docs/debts-design.md`: sections 4, 6, 9 and 13 say that a ledger entry has an account (`0018`, protocol 9, `accountId`).
- `README.md`: "Budgeting" names accounts and transfers; the documentation table and `docs/site/develop.mjs` ("Design notes")
  list this file.

## 16. Choices, and why

1. **The main account is local id 1 with a fixed `sync_id`, fixed stamp and fixed dates.** One account on both devices with no
   merge code, and `DEFAULT 1` moves every old row without a write.
2. **The main account can never be archived or deleted** (not "the last active one"). The rule is safe on two devices that
   cannot see each other (section 5.1).
3. **The opening balance is a dated movement; earlier rows still count.** Nothing is ever refused or silently left out.
4. **Names are checked on this device only, with no unique index and no merge in a sync.** A typing mistake is caught; two
   devices that pick the same name keep two accounts, because merging accounts would add up money that is in two places.
5. **The account sees the whole bill you paid and nothing of a bill a friend paid, and ledger entries carry an account.** This
   is the one addition to the decisions given: `ledger_entries.account_id`. With it a balance equals the bank's; without it,
   every repayment would be missing. (The smaller design, your part only and no ledger column, gives a balance that is off by
   what friends owe until they pay; if that is preferred, drop the ledger column and use `transaction_costs.cost_minor` for
   every expense.)
6. **Net worth includes what friends owe.** It is the person's money; and it makes net worth the same under both designs of 5.
7. **A payment of a recurring item whose account is archived goes to the main account**, rather than refusing the payment or
   rewriting items when an account is archived (which would make stamps and conflicts).
8. **Rows of an account deleted on the other device move to the main account** with a stamp both devices work out, as payments
   lose the link to a deleted recurring item. Nothing is lost and no sync is stopped by an honest device.
9. **No new error code.** `InvalidInput`, `NotFound` and `NameExists` cover every refusal; the screens hide what would fail.
10. **No account field with one account**, and the net worth card only once accounts are in use.

## 17. Open points

A main account that can be changed to another. A `client_uuid` on transfers (a retry from a script can make two). A running
balance in the transactions list. An export limited to one account. The statement of a very large account is read a month at a
time; a year at once was not measured. Whether a credit card should show "you owe" instead of a negative balance.
Reordering accounts on the phone (today the computer only: drag, or `Alt` and the arrow keys).

## 18. Work packages

Package 1 first (the others build on its names). Then 2, 3, 4 and 5 can run together: each needs only package 1. 6 last.

| # | Package | Files |
|---|---|---|
| 1 | Schema, Rust core, Tauri commands, their tests | `src-tauri/migrations/0018_accounts.sql`, `src/migrate.rs`, `src/money.rs`, `src/accounts.rs`, `src/accounts/transfers.rs`, `src/accounts/*_tests.rs`, `src/transactions.rs`, `src/recurring.rs`, `src/debts/ledger.rs`, `src/debts/codes.rs`, `src/commands.rs`, `src/lib.rs`, `build.rs`, `src/schema_tests.rs` |
| 2 | Sync | `src/sync/wire.rs`, `src/sync/engine.rs`, `src/sync/stamp.rs`, `src/sync/engine_tests.rs`, `src/sync/flow_tests.rs`, `src/sync/foundation_tests.rs`, `docs/sync-design.md` |
| 3 | API routes, CLI, recorded docs | `src/api/accounts.rs`, `src/api/accounts_tests.rs`, `src/api/routes.rs`, `src/api/read.rs`, `src/api/write.rs`, `src/api/debts.rs` (the `accountId` arguments), `cli/schema.json`, `cli/tests/docs.rs`, `docs/site/commands.json`, `docs/site/examples.json` |
| 4 | Currency, backup, export, report, seed | `src/currency.rs`, `src/backup.rs`, `src/backup/accounts_tests.rs`, `src/export.rs`, `src/export/xlsx.rs`, `src/export/accounts_tests.rs`, `src/report.rs`, `src/report/layout.rs`, `seeds/dev.sql`, `src/seed.rs` |
| 5 | UI and texts | `ui/src/pages/AccountsPage.tsx`, `pages/index.ts`, `App.tsx`, `components/AccountModal.tsx`, `TransferModal.tsx`, `BalanceChart.tsx`, `AppSidebar.tsx`, `NavIcon.tsx`, `QuickAdd.tsx`, `EntryModal.tsx`, `SettleModal.tsx`, `BillAcceptModal.tsx`, `ReviewModal.tsx`, `pages/TransactionsPage.tsx`, `RecurringPage.tsx`, `DashboardPage.tsx`, `ExportPage.tsx`, `api/accounts.ts`, `api/transfers.ts`, `api/transactions.ts`, `api/recurring.ts`, `api/debts.ts`, `api/dataExport.ts`, `api/reportLabels.ts`, `lib/accountsView.ts`, `lib/money.ts`, `keys/shortcuts.ts`, `index.css`, `i18n/*.ts` |
| 6 | Guide | `docs/site/guide.mjs`, `docs/site/shots.mjs`, `docs/site/images/`, `docs/site/develop.mjs`, `README.md`, `docs/debts-design.md`, this file (status line, section 17) |

Overlap to know about: packages 2, 3 and 4 only read what package 1 wrote in `transactions.rs`, `recurring.rs` and
`debts/ledger.rs`. Package 4's seed needs the migration only. Package 3 records the docs last, after 4 (the tour runs on the
demo data). Everyone runs `npm run check` before handing over.

## 19. Tests

**Package 1** (`accounts/table_tests.rs`, `lifecycle_tests.rs`, `balance_tests.rs`, `transfers_tests.rs` (sections 5.2 and 5.3),
`command_tests.rs`; `accounts/fixtures.rs` has `snapshot` and `assert_refused`, which prove that a refusal wrote nothing)
- The migration on an empty database and on one with rows at version 17: one main account with the constants of 4.3, every
  transaction, recurring item and ledger entry in it, no stamp and no `updated_at` moved, no history row for it.
- Every `CHECK`, index and trigger of 4.2 to 4.6; stamps on a real change only; markers on delete; triggers standing aside.
- Section 5.1 row by row, with 5.5 items 1 to 5, 12 (local part), 16, 20.
- Section 5.2 and 5.3 row by row for the local events, with 5.5 items 6, 7, 8.
- The view against a recomputation from random rows (fixed seeds): balance on any day, change this month, last activity,
  summary, statement (`balanceBefore` + lines = `balanceAfter`; the sum over all accounts of all transfers is 0), history.
  5.5 items 17, 18, 19.
- `parse_signed_amount`: accepted and refused forms. The commands list test (`COMMANDS` in two places).

**Package 2** (`sync/engine_tests.rs`, `flow_tests.rs`)
- Wire: every field of the two new rows refused when wrong; unknown fields refused; the main account archived refused.
- Two devices: a first copy; random edits of accounts and transfers from the fixed seeds end equal; 5.5 items 3 (sync part), 9,
  10, 11, 12, 13, 14; a row in an archived account arrives and stays; conflicts list the new fields; a sync cut at every message.
- A protocol 8 peer is told to update. Tables that must never be sent are still not sent.

**Package 3** (`api/accounts_tests.rs`, `cli/tests`)
- Each route: scope (a read token cannot write), arguments refused when unknown or of the wrong kind, answers of section 6.
- The refusals as status and code: main account archive `400`, taken name `409`, missing id `404`, transfer to itself `400`.
- `--opening-balance=-120.00` reaches the app; `accounts delete` and `transfers delete` ask for `--yes`.
- `schema.json` and the recorded docs are up to date (the existing tests fail until they are).

**Package 4**
- Currency: 5.5 item 15; a transfer and an opening balance converted with the rounding of the others; `Conversion` counts.
- Backup: a round trip with accounts and transfers; 5.5 item 21; the tables list test.
- Export: JSON, CSV and Excel carry the account; a limited export names only its accounts and has no balance, number or
  transfer; cells that look like formulas in an account name or note are written as text; exporting changes nothing.
- Report: the accounts table adds up to `Summary`; absent when a category limit is set or accounts are not in use.
- Seed: loads for every day of `DAYS`, passes `accounts::check`, shows every account state.

**Package 5** (`*.test.ts` beside the code)
- `accountsView`: `several`, `defaultAccountId` (last used, gone, archived), `accountName`. `signedAmountString`.
- Zod schemas accept the shapes of section 6 and refuse a number where money is text.
- The i18n test (every key in seven files, same placeholders); the shortcuts test with `goAccounts`.
- By hand, computer and phone widths: section 5.4 for both dialogs; one account shows no account field anywhere.

**Package 6**: `npm run check:docs`.

## 20. Round two: closer to Firefly III

Asked by the owner after the first round was built. Section numbers 1 to 19 stay; where this section disagrees with them, this
section wins. Migration `0019_account_details.sql`. Sync protocol **10**.

### 20.1 What changes

1. **Three kinds of entry, as Firefly's withdrawal, deposit and transfer.** The add dialog on the transactions page (and the quick
   add) has three tabs: **expense**, **income**, **transfer**. The expense tab lists only expense categories, the income tab only
   income categories, the transfer tab holds the fields of `TransferModal`. An edit opens on the tab of the row and shows no tabs.
2. **Income names its account.** On the income tab (transaction and recurring) the account field is always shown, also with one
   account, and is labelled "into account". An expense keeps the `several` rule of section 13.
3. **Transfers are on the transactions page.** Tabs above the list: **all**, **expenses**, **income**, **transfers**. "all" is
   what the page shows today (income and expenses). "transfers" lists the transfers (date, from, to, amount, note) with the date
   and account filters and the pager; a row opens the transfer dialog.
4. **Recurring items are two lists**: recurring expenses and recurring income, each with its own title, count and monthly total.
   The recurring dialog has two tabs (expense, income); the income tab has the account field of point 2.
5. **Income moves to the accounts page**, as Firefly keeps revenue under Accounts. The accounts page has two tabs: **accounts**
   and **income**. The income tab is the income half that the categories page had (income categories, their budgets, what came
   in this month, add, rename, delete, reorder). The categories page keeps expenses and history.
6. **More of what a Firefly account has** (20.2): BIC, notes, a group, "include in net worth", a virtual balance; on the account's
   page what was spent and what came in by category for the period shown, a month to month navigation, and "add" buttons that
   open the dialog of point 1 with this account already chosen.

Not built, with the reason: a currency per account, liabilities and piggy banks (not goals, section 1); expense and revenue
counterparty accounts (categories do that job here); reconciliation (it needs a fourth kind of movement; correct the opening
balance instead); attachments on an account (the document links table would have to be rebuilt); the "shared" role and a
credit card's payment day (the kind `CHECK` of 0018 cannot be widened without rebuilding a table four others point at);
recurring transfers; rules.

### 20.2 The rows (migration 0019)

`ALTER TABLE accounts ADD COLUMN` only, no table is rebuilt:

| Column | Type | Rule |
|---|---|---|
| `bic` | `TEXT` | `NULL` or 1 to 20 characters, no control character, trimmed |
| `notes` | `TEXT` | `NULL` or 1 to 1000 characters; line breaks allowed, no other control character |
| `group_name` | `TEXT` | `NULL` or 1 to 50 characters, a name as an account name |
| `include_net_worth` | `INTEGER NOT NULL DEFAULT 1` | 0 or 1 |
| `virtual_balance_minor` | `INTEGER NOT NULL DEFAULT 0` | within ±999999999999 |

Each has its `CHECK` in the column definition. The account triggers that list columns (`updated_at`, sync stamp) and the three
audit triggers (`trg_accounts_audit_insert`, `_update`, `_delete`: the history) are dropped and made again with the five new columns,
exactly as 0018 did for the tables it touched; without that, a change of a detail alone would leave no row of history. The main account keeps its constants: the
new columns have their defaults there, so two devices still hold one identical row and nothing is stamped.

### 20.3 The numbers

- `balance` stays what the account really holds (section 3). The virtual balance never enters it, a statement, a chart or net worth.
- `available` = `balance` + virtual balance: what the account can still pay, for example a credit card with a limit of 1000.00
  (virtual balance "1000.00") and a balance of "-240.00" has "760.00" available. Shown only when the virtual balance is not 0.
- Net worth (`Summary.inAccounts`, `netWorth`) adds only accounts with `include_net_worth = 1`. `Summary.accounts` and `inUse`
  count every account as before. The report's accounts table and its net worth line follow the same rule: an excluded account is
  listed with its balance and marked, and is left out of the net worth line.
- Currency change: `virtual_balance_minor` is converted as the opening balance is (section 8).

### 20.4 Core

```
Account       gains  bic: string|null, notes: string|null, group: string|null, includeInNetWorth: bool,
                     virtualBalance: "1000.00", available: "760.00"
NewAccount    gains  bic?, notes?, group?, includeInNetWorth? (true), virtualBalance? ("0.00")
AccountPatch  gains  bic? (null clears), notes? (null clears), group? (null clears), includeInNetWorth?, virtualBalance?
Breakdown     { from, to, expenses: [Share], income: [Share], expenseTotal: "410.00", incomeTotal: "3200.00" }
Share         { categoryId: string, name: string, amount: "137.00" }      // largest first, then by name
```

- `accounts::breakdown(conn, id, from, to) -> Breakdown`, Tauri command `account_breakdown(id, from, to)`: the transactions of the
  account in the period (both days included) that moved it (`paid_by IS NULL`, as the view), added up by category. Transfers and
  ledger entries are not in it. An unknown id is `NotFound`; `from` after `to` is `InvalidInput`.
- `transactions::Filter` gains `kind: Option<"INCOME"|"EXPENSE">` (JSON `kind`): only transactions whose category has that type.
- New Tauri command `list_transfers(filter)` over `transfers::list` (the transfers tab).
- Editing an archived account stays refused. The new fields follow state machine 5.1 like the old ones: a save that changes
  nothing makes no stamp.

### 20.5 Sync, backup, export, report, demo data

- `AccountRow` gains `bic`, `notes`, `group`, `includeNetWorth` (0 or 1), `virtualBalanceMinor`; `parse_row` checks them as 20.2;
  the main account's row may carry any of them. `PROTOCOL` 10; a protocol 9 peer is told to update. Conflict facts gain `bic`,
  `notes`, `group`, `includeNetWorth`, `virtualBalance`.
- Backup: nothing to list (same tables); a round trip keeps the five fields. A backup made at schema 18 restores with the defaults.
- Currency: `Conversion.accounts` counts the accounts that have an opening balance or a virtual balance that is not 0 (an account with
  both counts once); `apply_recorded` does the same (section 20.3).
- Report: the accounts table lists every account; one with `include_net_worth = 0` is marked with a new `ReportLabels` field
  `notInNetWorth` ("(not in net worth)" when the caller leaves it out, as for the four labels of section 10) and is left out of the net
  worth line, which is `accounts::summary`'s number for the last day. The virtual balance is not in the report.
- Export: JSON `accounts` and the Excel `Accounts` sheet gain the five fields (not in a limited export, section 10). The JSON has no
  `available`: it is worked out from the balance and the virtual balance, which are both in the file. The Excel sheet gets BIC, Group,
  Virtual balance, In net worth and Notes after the Balance column.
- Demo data: the credit card gets a virtual balance and a BIC; savings gets a group and a note; the archived account is left out
  of net worth.

### 20.6 Command line and API

- `accounts.create`, `accounts.update`: `bic`, `notes`, `group` (`Kind::String`), `includeInNetWorth` (a boolean, as the existing
  boolean arguments), `virtualBalance` (`Kind::String`, signed, as `openingBalance`).
- `accounts.breakdown` (read): `id`, `from`, `to` (default this month). `meowny accounts breakdown 2`.
- `transactions.list`: `kind` (`income` or `expense`, any case on the command line).
- The recorded docs and `schema.json` are made again.

### 20.7 The screens

All in the app's existing look (lower-case words, the mono face, hairline boxes, `SectionTabs` for tabs, `Modal`, `Dropdown`,
`DatePicker`, the row parts of `RowParts`). One page serves computer and phone; every new part must be right at 360 px wide
and at 1280 px.

- **Add dialog** (`TransactionModal`): `SectionTabs` under the title with expense, income, transfer; the tab chosen last is not
  remembered (it opens on expense; from an account's page "add income" opens on income, "transfer" on transfer). Changing tab
  keeps what both tabs have (amount, date, description/note) and drops the rest. Split is on the expense tab only. Keys: the
  existing shortcut opens the dialog; `Alt+1`, `Alt+2`, `Alt+3` choose a tab. With no income category the income tab says so and
  links to the income tab of the accounts page.
- **Transactions page**: the four tabs of 20.1 point 3 above the filters. On "transfers" the category filter and the search are
  hidden. With `several` false the transfers tab is hidden (there is nothing to move money between).
- **Recurring page**: two sections, expenses first; an empty section shows its empty line, not nothing.
- **Accounts page**: tabs accounts and income. List rows are grouped under their group's name when any account has a group
  (accounts without one under "no group", last); reorder works inside a group. A row shows "available" under the balance when
  20.3 says so, and a small mark when it is left out of net worth.
- **Account dialog**: name, type, number, opening balance and date as now; below them, shut behind "more": BIC, group (a text
  field that offers the groups in use), virtual balance, "count in net worth" (a switch, on), notes.
- **Account's page**: under the chart a month navigation (`MonthNav`) that sets the period of the statement and of two small
  lists side by side (stacked on the phone): "spent by category" and "came in by category", each a row per category with a bar
  and the amount, and its total. The title row has "add expense", "add income", "transfer". The notes show under the title.
- **Categories page**: expenses and history only.

### 20.8 Tests

Each package tests its edge cases and odd transitions as section 19 asks: the new fields at their bounds and refused beyond;
`null` clears; a patch that changes nothing makes no stamp; net worth with every account excluded, with the main account
excluded, with an archived one excluded; `available` with a negative virtual balance; a breakdown of an empty period, of a
period with only transfers, with a split bill a friend paid (not counted), with a future day; `kind` with a category that
changed type; sync of each new field in both directions and a protocol 9 peer; a conversion of the virtual balance at the
bounds; the dialog's tab changes (what is kept, what is dropped, an edit has no tabs, double submit ignored).

## 21. Round three: a currency per account

Asked by the owner: an account in lei beside accounts in euro. Section 1 named this "not a goal"; this section replaces that line and
section 20.1's "a currency per account" in the not-built list. Migration `0020_account_currency.sql`. Sync protocol **11**.

### 21.1 The idea

- The app keeps **one currency for budgets, statistics, the dashboard and net worth**: the app's currency (`settings.currency`),
  called the *main currency* here. Nothing in budgets, statistics, categories, debts or the report's income and expense parts changes.
- An account may be in **another currency** (a *foreign account*). Its balance, its statement, its chart and its breakdown are in
  its own currency.
- A transaction in a foreign account has **two amounts**: `amount` in the main currency, which is what every budget and statistic
  counts, exactly as before; and `nativeAmount`, what really left or reached the account, in the account's currency. In a
  main-currency account there is only `amount`. (Firefly calls this the foreign amount.)
- A transfer between two accounts with different currencies has two amounts: `amount` leaves the first account in its currency and
  `toAmount` reaches the second in its currency.
- A foreign account has a **rate**, typed by the person, never fetched: how many units of its currency one unit of the main currency
  buys ("1 EUR = 5.00 RON" is rate "5.00"). It is used for two things only: net worth (the balance turned into the main currency)
  and the amount the forms suggest. No history of rates is kept; changing the rate changes no stored amount.
- All currencies have two decimals in this app (as the app's own currency has).

Limits, chosen to keep it small, each named where it bites:
- The main account is always in the main currency.
- Recurring items and ledger entries (debts) live only in main-currency accounts: naming a foreign account for them is refused.
- The currency of an account can be changed only while nothing points at it (`entries == 0`).
- The app's own currency cannot be changed while a foreign account exists (`shortcut:` in `currency::change`: refuse, do not convert).

### 21.2 The rows (migration 0020)

`ALTER TABLE … ADD COLUMN` only:

| Table | Column | Rule |
|---|---|---|
| `accounts` | `currency TEXT` | `NULL` = the main currency (it follows a change of the app's currency by itself). Otherwise three capital letters (`GLOB '[A-Z][A-Z][A-Z]'`). |
| `accounts` | `rate TEXT` | `NULL` when `currency` is `NULL`, required otherwise: a decimal the app's rate parser accepts, greater than 0 (the `CHECK` only ties it to `currency`; the parser is in Rust) |
| `transactions` | `native_amount_minor INTEGER` | `NULL`, or 0 to 999999999999 (as `amount_minor`) |
| `transfers` | `to_amount_minor INTEGER` | `NULL`, or 1 to 999999999999 |

`CHECK (id <> 1 OR currency IS NULL)` on accounts cannot be added to an existing table, so the rule for the main account is in Rust,
in `accounts::check` and in a trigger (`trg_accounts_main_currency`, `RAISE(ABORT)` unless `applying`). The triggers of `accounts`,
`transactions` and `transfers` that list columns (`updated_at`, sync stamp, audit) are dropped and made again with the new columns,
as 0019 did. The view `account_movements` is dropped and made again:

- a transaction moves its account by `native_amount_minor` when the account is foreign **and** the column is not `NULL`, else by
  `amount_minor` (so a stale native amount on a row that ended in a main-currency account is harmless, and a foreign row that
  somehow has none still counts);
- `TRANSFER_IN` moves the `to` account by `COALESCE(to_amount_minor, amount_minor)`; `TRANSFER_OUT` by `amount_minor`;
- the opening balance and ledger entries as before.

### 21.3 The rules

A row "is foreign" when its account's `currency` is not `NULL`. `foreign(a, b)` for a transfer: the two accounts' currencies differ
(`NULL` equals `NULL`; `NULL` differs from 'RON'; 'RON' equals 'RON').

| What | Rule | Refusal |
|---|---|---|
| create account | `currency` left out, or equal to the main currency (any case): stored `NULL`, `rate` must be left out. Another code: `rate` required | `InvalidInput` |
| update account | `rate` may change at any time for a foreign account; a rate for a main-currency account is refused. `currency` may change only when `entries == 0`; to a foreign code needs a rate (in the patch or already there); to the main currency clears the rate | `InvalidInput` |
| the main account | `currency` and `rate` are refused | `InvalidInput` |
| create transaction in a foreign account | `nativeAmount` required ("0.01" to the maximum; "0.00" allowed exactly where `amount` allows it); `amount` required as always | `InvalidInput` |
| create transaction in a main-currency account | `nativeAmount` must be left out | `InvalidInput` |
| update transaction | staying in a foreign account: `nativeAmount` may change. Moving into a foreign account (from any account): `nativeAmount` required in the same patch. Moving into a main-currency account: `nativeAmount` must be left out and the stored one is cleared. A patch with `nativeAmount` for a row that is and stays in a main-currency account | `InvalidInput` |
| create transfer | currencies differ: `toAmount` required. Same currency: `toAmount` must be left out | `InvalidInput` |
| update transfer | Look at the two sides as they are after the patch. Their currencies differ: the row must end with a `toAmount`, so it is required in the patch when the row has none (a side just changed so that they now differ); when the row has one it may be left out (kept) or changed. Their currencies are equal: a `toAmount` in the patch is refused and a stored one is cleared | `InvalidInput` |
| recurring item, ledger entry, settle, accepting a bill code | the account named must be in the main currency. Paying a recurring item whose account became foreign cannot happen (currency changes only with no entries) | `InvalidInput` |
| split bill in a foreign account | allowed: the split works on `amount` (main currency); the account moves by `nativeAmount` when you paid and by nothing when a friend paid (section 3) | |
| change of the app's currency | refused while any account, archived or not, is foreign | `InvalidInput` |
| archive, restore, delete, reorder, groups, details | as before; the virtual balance and the opening balance are in the account's currency | |

Numbers:
- `Account.balance`, `changeThisMonth`, `available`, `openingBalance`, `virtualBalance`, the statement, history and breakdown of a
  foreign account are in its currency.
- `Account.balanceInMain` = `balance` turned into the main currency at the account's rate (`balance / rate`, half away from zero, to
  the cent); for a main-currency account it equals `balance`.
- `Summary.inAccounts` = the sum of `balanceInMain` over the accounts that count in net worth. `netWorth` as before on top of it.
- A transfer between different currencies is not zero-sum in numbers; each side moves by its own amount.

### 21.4 Core (`src-tauri/src/accounts.rs`, `accounts/transfers.rs`, `transactions.rs`)

```
Account       gains  currency: "RON" (always a code: the main currency's for a main-currency account), foreign: bool,
                     rate: "5.00"|null, balanceInMain: "12.34"
NewAccount    gains  currency?, rate?
AccountPatch  gains  currency?, rate?, balance?      // balance: see 21.5
Transaction   gains  nativeAmount: "113.74"|null
NewTransaction, TransactionPatch gain  nativeAmount?
Transfer      gains  toAmount: "40.20"|null
NewTransfer, TransferPatch gain  toAmount?
Line (statement) amounts are in the account's currency; Statement gains currency: "RON"
Breakdown gains currency: "RON"; its amounts are in the account's currency
```

- `accounts::require_main_currency(conn, account_id)` for the recurring, ledger, settle and bill-code paths.
- `accounts::to_main(minor, rate)` (divide by the rate, half away from zero, sign kept) built on the existing rate parser of
  `currency.rs` (`parse_rate`, `convert`): the inverse rate is the same fraction upside down. A rate of 0 or one the parser refuses
  is `InvalidInput`.
- `accounts::check` also: the main account's currency is `NULL`; every account has a rate exactly when it has a currency.

### 21.5 Set the balance

"I have 1662.90 on this account today": `AccountPatch.balance` (and `accounts.update --balance`, and a field in the edit dialog).
The core sets `opening_balance := opening_balance + (balance − current balance today)`, so the account holds exactly that today and
every earlier balance moves by the same amount. It is in the account's currency, signed, within the opening balance's bounds after
the change; it cannot be given together with `openingBalance` (`InvalidInput`); a `balance` equal to the current one writes nothing.

### 21.6 Sync, currency, backup, export, report, demo data

- Wire: `AccountRow` gains `currency`, `rate` (both optional, null for a main-currency account; `parse_row`: a code of three capitals,
  a rate the parser accepts, both or neither, neither for the main account); `TransactionRow` gains `nativeAmountMinor` (optional,
  0 to `MAX_MINOR`); `TransferRow` gains `toAmountMinor` (optional, 1 to `MAX_MINOR`). `PROTOCOL` 11. Facts: `currency`, `rate`,
  `nativeAmount`, `toAmount`. A row moved to the main account because its account was deleted on the other device (section 7.4) keeps
  its stored native amount; the view ignores it there.
- Two devices can end with a foreign account whose currency equals the app's (one changed the app's currency while the other made
  the account). It stays a foreign account with its rate; nothing breaks, and the person can fix it by hand.
- `currency::change` and the recorded conversion: refused while a foreign account exists; with none, as before.
- Backup: a round trip keeps the four columns; a backup of schema 19 restores with `NULL`s.
- Export: JSON accounts gain `currency`, `rate`, `balanceInMain`; transactions gain `nativeAmount` and transfers `toAmount`; CSV gets
  two columns at the end, `native_amount` and `account_currency`; Excel the same on the transactions sheet, `Currency` and `Rate`
  on the accounts sheet, `Arrives` on the transfers sheet. A limited export gives an account its `currency` beside id, name and kind.
- Report: in the accounts table a foreign account's balance and change carry its code ("RON 120.00"); the net worth line is in the
  main currency (21.3).
- Demo data: one foreign account ("Lei account", RON, rate 5.00) with a few transactions and a transfer in each direction.

### 21.7 Command line and API

- `accounts.create`, `accounts.update`: `currency` (`Kind::String`, any case), `rate` (`Kind::String`); `accounts.update`: `balance`
  (`Kind::String`, signed).
- `transactions.create`, `transactions.update`: `nativeAmount` (`Kind::String`).
- `transfers.create`, `transfers.update`: `toAmount` (`Kind::String`).

### 21.8 The screens

- **Account dialog**: under type, **currency** (a short text field, three letters, the main currency by default; disabled with a
  line of why when the account has entries or is the main account). When it is not the main currency: **rate**, written as a
  sentence, "1 EUR = [5.00] RON". In an edit: **balance today**, empty by default, with the hint "sets the opening balance so the
  account holds this today".
- **Money**: an amount of a foreign account is always shown with its own currency (the locale's format for that currency). The
  accounts list shows the balance in the account's currency and, under it, small, the value in the main currency ("≈ €24.00");
  "in accounts" and net worth are in the main currency.
- **Add dialog**, expense and income tabs, with a foreign account chosen: the amount field is labelled with the account's currency
  ("amount (RON)") and is the native amount; under it a second field "in EUR" filled from the rate while the person has not typed
  in it, and theirs once they have. With a main-currency account: one amount field, as now.
- **Transfer** between different currencies: "amount (EUR)" leaves, and a second field "arrives (RON)" filled from the two rates
  until typed in.
- **Lists**: the transactions page shows the main-currency amount as now and, beside it, small, the native amount with its code. An
  account's page (statement, chart, breakdown, title) is in the account's currency.
- **Recurring and debts dialogs**: the account field lists only main-currency accounts.
- **Settings › currency**: with a foreign account, the change button is disabled with a line of why.

### 21.9 Tests

As sections 19 and 20.8 ask, with the odd transitions named: a rate of "0", "-1", "abc", "5,00"; a currency of two letters, in
lower case (accepted, stored in capitals), equal to the main currency (stored `NULL`); a currency change with one entry refused and
after the entry is moved away allowed; the main account refused; a transaction moved main→foreign without and with `nativeAmount`,
foreign→main (cleared), foreign→foreign (kept unless given); a native amount given for a main-currency account; a transfer made
between RON and EUR, then one side changed so both are EUR (the second amount cleared) and back (required again); a transfer RON→RON
(no second amount); net worth with a foreign account at several rates, excluded, archived, negative; `balance` set to the current
value (nothing written), to a value beyond the bounds, together with `openingBalance`, on an archived account, on a foreign account,
with future-dated rows (they are not part of "today"); recurring, ledger, settle and bill code refused for a foreign account;
the app's currency change refused with a foreign account, archived or not, and allowed again after it is deleted; sync of each new
field both ways, the main account with a currency refused, a protocol 10 peer told to update.
