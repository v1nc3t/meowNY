# Stashes design

How stashes work: money set aside inside an account for something you save for. Section numbers are cited in the code; keep them.
User side: the [guide](https://v1nc3t.github.io/meowNY/guide.html#stashes). Status: being built (migration `0021`, sync protocol 12).

## 1. What it has to do

- A stash is a goal you save for: a holiday, a new laptop, a rainy-day fund. It has a name, **one account it lives in (chosen when it
  is made, always required)**, an optional target amount, an optional target date, notes, an order in the list, and it can be archived.
- Money in a stash **never leaves its account**. A stash only says "this much of the account is set aside". The account's balance,
  statement, chart, net worth, budgets and statistics do not change when money goes into or out of a stash.
- Money goes in and comes out by **entries**: an add or a take, each with an amount, a date and an optional note. The history of a
  stash is its list of entries; what it holds (**saved**) is their sum.
- An add can bring the money from another account in the same step (a transfer from that account into the stash's account is made
  with it); a take can send the money on to another account (a transfer out). The entry remembers the transfer.
- What an account holds that no stash has set aside is **free**. You cannot set aside more than is free, more than the target, or take
  out more than is saved.
- A stash shows how far it is (saved of target, percent), what is left, and, with a target date, how much to save each month.
- Everything follows sync, currency changes, backups, exports, the report, the demo data and the CLI. No network. No new dependency.
- Not goals: a stash over several accounts; money set aside automatically by rules or by every transfer into the account; a stash
  that moves money by itself (an add never moves money unless the person asks for the transfer); attachments on a stash; a stash in
  a currency other than its account's; recurring adds (a recurring transfer is not a feature either, `accounts-design.md` 20.1).

## 2. The pieces

| Name | Where |
|---|---|
| Stash | `stashes` |
| Entry: one add (`amount_minor > 0`) or take (`amount_minor < 0`) | `stash_entries` |
| The transfer that came with an entry | `stash_entries.transfer_id` (`ON DELETE SET NULL`) |
| Saved, set aside, free, left, per month | Worked out, stored nowhere (section 3) |
| Rust | `src-tauri/src/stashes.rs` and `src-tauri/src/stashes/entries.rs` |
| UI | `ui/src/pages/StashesPage.tsx`, `components/StashModal.tsx`, `components/StashMoneyModal.tsx`, `api/stashes.ts`, `lib/stashView.ts` |

## 3. The numbers

Integers in minor units. A stash is in **its account's currency** (`accounts-design.md` 21): for a foreign account every amount of the
stash (target, entries, saved) is in that account's currency.

- **saved(S)** = `SUM(amount_minor)` of the entries of S, whatever their date. It can be below 0 or above the target only after a
  sync joined edits that were each fine on their own device (section 7.6); the screens show it as it is.
- **held(S)** = `max(saved(S), 0)`: what a stash takes away from its account. A stash that went below 0 never adds money to an
  account.
- **set aside(A)** = sum of `held` over every stash in account A, archived stashes too.
- **free(A)** = balance of A today (`accounts-design.md` 3) − set aside(A). Can be below 0 (spending after money was set aside, or a
  sync); the screens then warn "the stashes hold more than the account has".
- **left(S)** = `max(target − saved, 0)` when S has a target; none without one.
- **percent(S)** = `floor(saved × 100 / target)` when S has a target and `saved > 0`, else 0 (with a target) or none (without). May be
  above 100. The bars cap at 100.
- **reached(S)** = S has a target and `saved >= target`.
- **months(S)** with a target date D and today T: `(year(D) × 12 + month(D)) − (year(T) × 12 + month(T))`. **per month** = none
  without a target, without a target date, or when left is 0; when `D < T` the stash is **overdue** and per month is none; otherwise
  `ceil(left / max(months, 1))` (to the cent, rounded up, so saving it each month reaches the target in time). Target this month:
  per month = left.
- **saved on a day** (chart, report): sum of the entries dated on or before that day.
- **In the main currency** (dashboard card): `held` turned into the app's currency with `accounts::to_main` at the account's rate;
  for a main-currency account it is the same number.

### 3.1 The delta rule (every local write)

Every local write that changes what a stash holds (add, take, edit an entry, delete an entry, move the stash to another account) is
checked by what it does, with `before` = saved now and `after` = saved after the write:

| Case | Guard | Refusal |
|---|---|---|
| `after > before` (money goes in) | `after − before <= free(A)` of the account **before** the write (for an add with a transfer: after the transfer is made, in the same database transaction); and, when S has a target, `after <= target` | `InvalidInput` |
| `after < before` (money comes out) | `after >= 0` | `InvalidInput` |
| `after == before` | none | |

`free(A)` here counts the stash itself with `held(before)` (it is part of `set aside`). Example: account 1000, stash X holds 300,
stash Y holds 200: free = 500; X may grow by at most 500.

Moving a stash to account B (section 5.1): `held(S) <= free(B)` (B does not yet count S), and B in the same currency as A.

A sync never checks these: each device checked its own writes (section 7.6).

## 4. The rows

Migration `0021_stashes.sql`, appended to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Triggers that read the clock use the form of
`0016`/`0020` (`+ offset_ms`). Order inside the file: 4.1 to 4.5. The header comment explains the migration as `0018` and `0020` do.

### 4.1 `sync_deleted` gets two kinds

Rebuild as `0018` section 4.1 did: drop every trigger that writes to `sync_deleted` (`trg_budgets_sync_delete`,
`trg_recurring_transactions_sync_delete`, `trg_transactions_sync_delete`, `trg_documents_sync_delete`, `trg_document_links_sync_delete`,
`trg_ledger_entries_sync_delete`, `trg_accounts_sync_delete`, `trg_transfers_sync_delete` — check with
`SELECT name FROM sqlite_master WHERE type = 'trigger' AND sql LIKE '%sync_deleted%'` on a database at version 20 that this list is
complete), rebuild the table with `'stashes'` and `'stash_entries'` added to `ck_sync_deleted_kind`, copy the rows, recreate the
unique index and every dropped trigger with its **latest** body (the newest migration that made it; `0018` for most of them).

### 4.2 `stashes`

```sql
CREATE TABLE stashes (
  id           INTEGER PRIMARY KEY,
  sync_id      TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_stashes_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp        TEXT,
  name         TEXT NOT NULL CONSTRAINT ck_stashes_name CHECK (length(name) BETWEEN 1 AND 50),
  account_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE NO ACTION,
  target_minor INTEGER CONSTRAINT ck_stashes_target CHECK (target_minor IS NULL OR target_minor BETWEEN 1 AND 999999999999),
  target_date  TEXT CONSTRAINT ck_stashes_target_date
    CHECK (target_date IS NULL OR (target_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(target_date) IS target_date)),
  notes        TEXT CONSTRAINT ck_stashes_notes CHECK (notes IS NULL OR length(notes) BETWEEN 1 AND 1000),
  archived     INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_stashes_archived CHECK (archived IN (0, 1)),
  position     INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX uq_stashes_sync_id ON stashes (sync_id);
CREATE INDEX idx_stashes_account ON stashes (account_id);
```

No unique index on the name (as accounts, `accounts-design.md` 16 choice 4). `position` counts from 0 over all stashes; lists sort by
`position, id`.

### 4.3 `stash_entries`

```sql
CREATE TABLE stash_entries (
  id           INTEGER PRIMARY KEY,
  sync_id      TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_stash_entries_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp        TEXT,
  stash_id     INTEGER NOT NULL REFERENCES stashes (id) ON DELETE CASCADE,
  amount_minor INTEGER NOT NULL CONSTRAINT ck_stash_entries_amount
    CHECK (amount_minor <> 0 AND amount_minor BETWEEN -999999999999 AND 999999999999),
  entry_date   TEXT NOT NULL CONSTRAINT ck_stash_entries_date
    CHECK (entry_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(entry_date) IS entry_date),
  note         TEXT CONSTRAINT ck_stash_entries_note CHECK (note IS NULL OR length(note) BETWEEN 1 AND 200),
  transfer_id  INTEGER REFERENCES transfers (id) ON DELETE SET NULL,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX uq_stash_entries_sync_id ON stash_entries (sync_id);
CREATE INDEX idx_stash_entries_stash ON stash_entries (stash_id, entry_date, id);
-- the foreign key action that clears the link looks entries up by transfer
CREATE INDEX idx_stash_entries_transfer ON stash_entries (transfer_id) WHERE transfer_id IS NOT NULL;
```

The sign says what the entry is: above 0 an add, below 0 a take. An entry never changes from one to the other (section 5.2).

### 4.4 Triggers

Copy the triggers of `transfers` (`0020`) and of `ledger_entries` (latest bodies), with these columns:

| Trigger | `stashes` | `stash_entries` |
|---|---|---|
| `trg_<t>_updated_at` (`AFTER UPDATE OF`, stands aside while applying, only on a real change) | `id, name, account_id, target_minor, target_date, notes, archived, position, created_at, updated_at` | `id, stash_id, amount_minor, entry_date, note, transfer_id, created_at, updated_at` |
| `trg_<t>_sync_insert` (`WHEN NEW.stamp IS NULL`) | yes | yes |
| `trg_<t>_sync_update` (new stamp only on a real change, `IS NOT` on each column) | `name, account_id, target_minor, target_date, notes, archived, position` | `stash_id, amount_minor, entry_date, note, transfer_id` |
| `trg_<t>_sync_delete` (marker of kind `'stashes'` / `'stash_entries'`, stands aside while applying) | yes | yes, **only while its stash still exists**: `AND EXISTS (SELECT 1 FROM stashes WHERE id = OLD.stash_id)`, as `trg_document_links_sync_delete` does for its document. Entries removed by the cascade of a stash deletion leave no marker: the stash's marker covers them, and the other device cascades too |
| `trg_<t>_audit_insert`, `_audit_update`, `_audit_delete` (`entity_type` = the table name, every column but `sync_id`, `stamp`) | yes | yes |

The link to a transfer, as `ledger_entries.transaction_id` is kept with `trg_transactions_sync_unlink` (`0016`):

```sql
-- the entries that came with a transfer lose the link when it is deleted (ON DELETE SET NULL): they get a stamp first, so the
-- other device learns that the link is gone
CREATE TRIGGER trg_transfers_sync_unlink
BEFORE DELETE ON transfers
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  -- tick the clock exactly as trg_transactions_sync_unlink (0016) does, then:
  UPDATE stash_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1)
   WHERE transfer_id = OLD.id;
END;
```

The `SET NULL` that follows is itself a write of `transfer_id`, so `trg_stash_entries_sync_update` and `trg_stash_entries_updated_at`
fire on it too. Do exactly what the ledger link does (`ledger_entries.transaction_id`, `ON DELETE SET NULL`, `trg_transactions_sync_unlink`
in `0016`, and the ledger triggers' `WHEN` clauses): read those first and copy their shape. Prove it with a test that deletes a linked
transfer and asserts the entry ends with `transfer_id IS NULL`, a stamp newer than before, and one history row for the change.

No trigger checks "the account is active", "the stash is not archived" or the delta rule: those are rules of the core and the API
(section 5), because a sync may rightly deliver rows that break them (section 7.6).

### 4.5 What the accounts gain

- `accounts::Account.entries` also counts the stashes in the account: an account with a stash is not deleted (the foreign key
  `NO ACTION` is the net). `accounts::delete`'s count gets the same `+ (SELECT count(*) FROM stashes WHERE account_id = a.id)`.
- `accounts::Account` gains `set_aside: String` (JSON `setAside`, decimal, never negative, in the account's currency, section 3) and
  `free: String` (JSON `free`, signed). Both are worked out in the same query as the balance.
- Nothing else of accounts changes: balance, statement, history, breakdown, summary and net worth ignore stashes.

## 5. State machines

Refusals answer with the existing `AppError` values only: `InvalidInput` (API `400 invalid_input`), `NotFound` (`404`), `NameExists`
(`409 name_exists`). No new error code: the screens do not offer what would be refused. A refused event changes nothing and makes no
stamp (prove it with `accounts::fixtures::snapshot`-style helpers).

### 5.1 The stash

States: `NONE`, `ACTIVE`, `ARCHIVED`, `DELETED` (marker).

| State | Event | Guard | Next | Effect, or what a refusal answers |
|---|---|---|---|---|
| `NONE` | create | name 1 to 50 characters after trimming, no control character (`categories::clean_name`) | `ACTIVE` | row at the end (`MAX(position) + 1`, 0 for the first); else `InvalidInput` |
| | | no `ACTIVE` stash has this name, ignoring case | | else `NameExists` |
| | | `accountId` given (required), the account exists and is `ACTIVE` | | else `InvalidInput` |
| | | target absent or "0.01" to "9999999999.99"; target date absent or a real date (past allowed: the stash is then overdue); notes absent or 1 to 1000 characters, line breaks allowed (as account notes) | | else `InvalidInput` |
| `ACTIVE` | update name, target, target date, notes | the same checks; the name check skips the stash itself; a new target must be `>= saved` (clearing it is always allowed) | `ACTIVE` | fields written, new stamp only on a change |
| `ACTIVE` | update `accountId` to the account it has | | `ACTIVE` | not a move: nothing checked, nothing written for it |
| `ACTIVE` | move to account B | B exists, is `ACTIVE`, has the same currency as the current account (`accounts::currencies_differ` is false), and `held <= free(B)` | `ACTIVE` | `account_id = B`; else `InvalidInput` |
| `ACTIVE` | reorder | the ids are exactly the `ACTIVE` stashes, each once | `ACTIVE` | only rows whose position changes are written; else `InvalidInput` |
| `ACTIVE` | archive | | `ARCHIVED` | `archived = 1`; entries untouched; what it holds stays set aside |
| `ACTIVE` | restore | | `ACTIVE` | answers the stash, nothing written |
| any but `NONE`/`DELETED` | delete | | `DELETED` | row and its entries removed (cascade), one marker for the stash; linked transfers stay |
| `ARCHIVED` | archive again | | `ARCHIVED` | answers the stash, nothing written |
| `ARCHIVED` | restore | no `ACTIVE` stash has its name | `ACTIVE` | `archived = 0`; else `NameExists` |
| `ARCHIVED` | update, reorder | | `ARCHIVED` | refused: `InvalidInput` (restore it first) |
| `ARCHIVED` | add | | `ARCHIVED` | refused: `InvalidInput` |
| `ARCHIVED` | take, edit an entry, delete an entry | the delta rule (3.1) | `ARCHIVED` | allowed, so an archived stash can be emptied |
| either | its account is archived | | same | nothing written. Adds are refused while the account is archived (the account must be `ACTIVE` for money to go in); takes, entry edits and deletes are allowed; the stash can be moved to an active account |
| either | its account is deleted here | | | cannot happen: refused while a stash points at it |
| any | currency conversion | | same | target and entries converted (section 8) |
| `NONE`, `DELETED` | any event by id | | same | `NotFound` |
| sync | rows and markers | | | section 7 |

### 5.2 An entry

States: `NONE`, `LIVE`, `DELETED` (marker, or gone with its stash).

| State | Event | Guard | Next | Effect, or what a refusal answers |
|---|---|---|---|---|
| `NONE` | add to S | S `ACTIVE`; its account `ACTIVE`; amount "0.01" to "9999999999.99"; date a real date, not after today (left out: today); note absent or 1 to 200 characters after trimming, line breaks allowed, empty means none; the delta rule with `after = before + amount` | `LIVE` | `amount_minor = +amount`; else `InvalidInput` |
| | add with `fromAccountId` F | F exists, is `ACTIVE`, differs from the stash's account, has the same currency | | first a transfer F → stash's account (amount, date, the entry's note), then the entry with `transfer_id` set, in one database transaction; then the delta rule (free counts the transfer). Any refusal writes neither |
| `NONE` | take from S | S `ACTIVE` or `ARCHIVED`; its account may be archived; amount, date and note as for an add; `after = before − amount >= 0` | `LIVE` | `amount_minor = −amount` |
| | take with `toAccountId` T | T exists, is `ACTIVE`, differs from the stash's account, same currency; the stash's account may be archived (money can still leave it) | | the entry, then a transfer stash's account → T (amount, date, note), linked, in one transaction |
| `LIVE` | edit amount, date, note | the same checks of amount, date, note; the kind never changes (an add stays an add: the new amount is a magnitude); the delta rule with `after = before − old + new`; adding money to an archived stash or one whose account is archived (`after > before`) is refused | `LIVE` | fields written; new stamp only on a change. The linked transfer is **not** changed (it is its own row; the screens say so) |
| `LIVE` | delete | the delta rule with `after = before − amount_minor` | `DELETED` | row removed, marker made. With `deleteTransfer: true` and a link: the transfer is deleted too, in the same transaction (its own marker); without, the transfer stays |
| `LIVE` | its transfer is deleted | | `LIVE` | `transfer_id` becomes `NULL`, one new stamp (4.4) |
| `LIVE` | its stash is deleted | | gone | cascade, no marker of its own |
| `NONE`, `DELETED` | edit, delete | | same | `NotFound` |

### 5.3 The dialogs

`StashModal` (make, edit a stash) and `StashMoneyModal` (add, take, edit an entry) follow `accounts-design.md` 5.4: `Modal` with
`pending`, toasts through `useToast`, `errorText`; "invalid" and "failed" are a toast and a return to `EDITING`. One addition: the money
dialog checks the delta rule **live** from the numbers it was opened with and shows why under the amount field (section 13.4); **done**
is disabled while the amount is empty, unparseable or breaks the rule. The core still checks (a race or a sync can change the numbers).

| State | Event | Guard | Next | Effect |
|---|---|---|---|---|
| `CLOSED` | open (make) | | `EDITING` | empty fields; account = the account of the page it was opened from, else `defaultAccountId` |
| `CLOSED` | open (edit) | | `EDITING` | fields hold the row |
| `CLOSED` | open (add / take) | | `EDITING` | amount empty, date today, note empty, no transfer |
| `CLOSED` | open (edit entry) | | `EDITING` | fields hold the entry; no tabs, the kind is fixed |
| `EDITING` | change tab (add ↔ take) | | `EDITING` | amount, date and note kept; the transfer choice dropped |
| `EDITING` | submit | a field is wrong | `EDITING` | error toast; nothing sent; fields kept |
| `EDITING` | submit | all right | `SAVING` | request sent; done disabled |
| `EDITING` | cancel, `Esc`, click outside | | `CLOSED` | nothing saved |
| `SAVING` | submit again | | `SAVING` | ignored (`if (pending) return`) |
| `SAVING` | answer ok | | `CLOSED` | the page reloads; an add that makes the stash reach its target shows the toast `stashes.reachedToast` |
| `SAVING` | answer error | | `EDITING` | error toast `errorText(error)`; fields kept |

An edit sends only the fields that changed; nothing changed closes the dialog and sends nothing.

### 5.4 Weird transitions the tests must cover

1. Archive twice; restore one never archived: answers the stash, no stamp.
2. Make a stash with no account, an unknown one, an archived one: refused. With one account: allowed (the field is always there).
3. Add exactly what is free; one cent more; add when free is negative; add to an archived stash; add when its account is archived.
4. Add exactly what is left to the target; one cent more; add to a stash without a target (only free limits it).
5. Take exactly what is saved; one cent more; take from a stash that went below 0 after a sync (refused: `after < 0`); take from an
   archived stash and from one whose account is archived (allowed).
6. Edit an add down below what was taken since (refused); edit a take up (more out) beyond saved (refused); edit an add up beyond free
   or target (refused); edit only the note of an entry when the stash is already above its target or below 0 after a sync (allowed:
   `after == before`).
7. Delete an add whose money was taken out since (refused: `after < 0`); delete a take (allowed only within free and target).
8. Lower the target below saved (refused); to exactly saved (allowed, reached); clear it (allowed).
9. Move a stash to an account in another currency (refused); to one with too little free (refused); to an archived one (refused); to
   the account it has (not a move, allowed); move a stash that holds 0 (always allowed when active and same currency).
10. Add with a transfer from the stash's own account, from an archived one, from one in another currency: refused, and no transfer was
    made. Add with a transfer where free is 0 before the transfer: allowed (the transfer makes the room).
11. Delete a linked transfer from the accounts side: the entry stays, its link clears, one stamp. Delete an entry with and without
    `deleteTransfer`.
12. Delete an account with a stash (refused); after the stash is moved or deleted (allowed).
13. Two stashes in one account: the second cannot take what the first holds.
14. Entry dated tomorrow (refused); dated long ago (allowed; it counts in saved).
15. Spend after setting aside: the balance falls below set aside, free is negative, nothing is refused but adds.
16. Name taken by an active stash (refused), by an archived one (allowed); restore an archived stash whose name was taken meanwhile.
17. Reorder with a missing, repeated, archived or unknown id: refused.
18. Every sync case of section 7.6.

## 6. Rust core

All in `src-tauri/src/stashes.rs` (stashes, numbers, history, summary) and `src-tauri/src/stashes/entries.rs` (add, take, edit, delete,
list). Tests in `src-tauri/src/stashes/*_tests.rs` (`table_tests.rs`, `lifecycle_tests.rs`, `entries_tests.rs`, `numbers_tests.rs`,
`command_tests.rs`). Ids cross as strings (`serialize_id`, `parse_id`); money as decimal strings (`format_amount`, `parse_amount`); JSON
is camelCase with `deny_unknown_fields` on inputs. `today` comes from `today_text(&vault)` in `commands.rs`.

### 6.1 Types

```
Stash      { id, name, accountId, account: string (the account's name), currency: "EUR" (the account's), archived: bool,
             accountArchived: bool, target: "1200.00"|null, targetDate: "2027-06-30"|null, notes: string|null,
             saved: "450.00" (signed), left: "750.00"|null, percent: 37|null, reached: bool, perMonth: "107.15"|null,
             overdue: bool, free: "1220.35" (the account's free, signed), entries: 12, lastEntry: "2026-11-02"|null }
NewStash   { name, accountId, target?, targetDate?, notes? }
StashPatch { name?, accountId?, target? (null clears), targetDate? (null clears), notes? (null clears) }
Entry      { id, stashId, kind: "ADD"|"TAKE", amount: "50.00" (always above 0), date, note: string|null,
             transferId: string|null, otherAccountId: string|null, otherAccount: string|null (the other side of the transfer),
             savedAfter: "500.00" }
NewEntry   { amount, date?, note?, fromAccountId? }      // add
NewTake    { amount, date?, note?, toAccountId? }        // take
EntryPatch { amount?, date?, note? (null clears) }
Detail     { stash: Stash, entries: [Entry] }            // entries newest first; savedAfter runs oldest first by (date, id)
HistoryPoint { month: "2026-10", saved: "450.00" }       // as accounts::HistoryPoint
Summary    { stashes: 3 (not archived), saved: "1850.00" (sum of held, in the app's currency), target: "4200.00" (sum of targets
             of active stashes with a target, in the app's currency), reached: 1 }
```

`entries` is the number of entries. `lastEntry` is the latest entry date. `perMonth`, `overdue`, `left`, `percent`, `reached` as
section 3. Money of a stash is in `currency`.

### 6.2 Functions and Tauri commands

| Function | Tauri command | Answer |
|---|---|---|
| `stashes::list(conn, account_id: Option<i64>, today)`: active by `position, id`, then archived by `position, id` | `list_stashes(accountId?)` | `Stash[]` |
| `stashes::get(conn, id, today)` | `get_stash(id)` | `Detail` |
| `stashes::create(conn, &NewStash, today)` | `create_stash(input)` | `Stash` |
| `stashes::update(conn, id, &StashPatch, today)` | `update_stash(id, patch)` | `Stash` |
| `stashes::archive(conn, id, today)` / `restore` | `archive_stash(id)` / `unarchive_stash(id)` | `Stash` |
| `stashes::delete(conn, id)` | `delete_stash(id)` | `null` |
| `stashes::reorder(conn, &[i64])` | `reorder_stashes(ids)` | `null` |
| `stashes::history(conn, id, year, today)` | `stash_history(id, year)` | `HistoryPoint[]` (up to 12, as `accounts::history`) |
| `stashes::summary(conn, today)` | `stashes_summary()` | `Summary` |
| `entries::add(conn, stash_id, &NewEntry, today)` | `add_to_stash(id, input)` | `Detail` |
| `entries::take(conn, stash_id, &NewTake, today)` | `take_from_stash(id, input)` | `Detail` |
| `entries::update(conn, entry_id, &EntryPatch, today)` | `update_stash_entry(id, patch)` | `Detail` |
| `entries::delete(conn, entry_id, delete_transfer: bool, today)` | `delete_stash_entry(id, deleteTransfer)` | `Detail` |

The four entry commands answer the whole `Detail` so the page redraws from one answer. Each new command is added to `COMMANDS` in
`commands.rs`, to `COMMANDS` in `src-tauri/build.rs`, to `generate_handler!` in `src-tauri/src/lib.rs`, and as `allow-<name>` in
`src-tauri/capabilities/default.json` (and `mobile.json` if it lists commands), exactly as the account commands are; the commands list
test proves the lists agree. `accounts::Account` gains `setAside` and `free` (4.5); `stashes::free(conn, account_id, today) -> i64`
(crate) is the one place that works out free; `accounts.rs` and `stashes.rs` use it or the same SQL.

Every write runs in one `conn.unchecked_transaction()` / `transaction()` (as `accounts::update` does), so a refusal after a partial
write (the transfer of an add) leaves nothing.

## 7. Sync

Protocol **12**. A protocol 11 peer is told to update, as now.

### 7.1 Tables and order

`wire::Table` gains `Stashes` and `StashEntries` (names `"stashes"`, `"stash_entries"`). `Table::ALL`, sending and applying order:
settings, categories, people, accounts, recurring, budgets, transactions, transfers, **stashes, stash entries**, ledger, documents,
links. `engine::LISTED` and `stamp::STAMPED_TABLES` gain both. `engine::is_empty` needs both empty.

### 7.2 Wire rows

```
StashRow      { syncId, stamp, name, account, targetMinor, targetDate, notes, archived, position, createdAt, updatedAt }
              // account: the account's sync id; targetMinor, targetDate, notes: optional (null)
StashEntryRow { syncId, stamp, stash, amountMinor, entryDate, note, transfer, createdAt, updatedAt }
              // stash: the stash's sync id; transfer: the transfer's sync id or null
```

Checks in `parse_row`, nothing trimmed or repaired: name as a category name; `account`, `stash` and `transfer` valid sync ids; target
absent or 1 to `MAX_MINOR`; a real date; notes as account notes; `archived` 0 or 1; `position` 0 to `MAX_POSITION`; `amountMinor` not 0
and within ±`MAX_MINOR`; `note` as `note_ok`; unknown fields refused.

### 7.3 Applying

- `Applier::stashes`: by `sync_id`, newer stamp wins the whole row; marker rules as accounts. **Never merged by name.** The account is
  resolved with `Applier::account` (here; or the main account with the later stamp when it has a marker here; else
  `Invalid("reference")`), as `transfers` does.
- `Applier::stash_entries`: the stash here → its id; the stash has a marker here → the entry is not taken (`kept_yours`, as a link to a
  deleted document); unknown → `Invalid("reference")`. The transfer: none; here → its id; marker here → `NULL` and the entry's stamp
  becomes the later of its own and the marker's; unknown → `Invalid("reference")`.
- Before the commit, the existing checks and `foreign_key_check`. No delta rule (7.6).

### 7.4 Deletions

Markers are applied in this order: ledger, transactions, **stash entries**, transfers, budgets, recurring, links, documents,
**stashes**, accounts last (read the code: insert the two new kinds where they keep "a row deleted in the same batch is gone before its
parent's deletion would stamp it").

- A **stash** marker newer than the row: delete the stash; its entries go by the cascade (the trigger stands aside while applying, so
  no marker for them).
- A **transfer** marker: before deleting the transfer, the entries linked to it take `stamp = max(stamp, M)` (as `unlink_entries` does
  for ledger entries and transactions); the `SET NULL` clears the link. Both devices end equal.
- An **account** marker (`empty_account`): stashes in it move to the main account like transactions:
  `UPDATE stashes SET account_id = 1, stamp = max(stamp, M) WHERE account_id = X`. Their amounts stay as they are, even when the
  deleted account had another currency (mixed units, as a rescued transfer; the person corrects them). The device that deleted the
  account does the same from its end (`Applier::account`).

### 7.5 Conflicts

Stashes and entries are ordinary rows in the conflict review. `engine::facts` gains: for a stash `name`, `account`, `target`,
`targetDate`, `notes`, `archived`; for an entry `stash` (its name), `kind`, `amount`, `date`, `note`. Their words are `sync.field.*`
texts (section 14); reuse existing keys (`sync.field.account`, `sync.field.note`, `sync.field.archived`, ...) where they exist.

### 7.6 What a sync may leave

The delta rule is checked on each device alone. Two devices may each add 300 to a stash with a target of 500, or each set aside the
free money of one account: after the sync the stash holds 600 (reached, 120 %) or the account's free is negative. Two devices may each
take out the last 100: saved is −100. Nothing is refused or repaired; the screens show it as it is (the bar full and "reached", the
account's warning, a stash below 0 shows its saved in red and offers only add). Why: a sync must never stop on rows an honest device
made, and a repair would need a stamp both devices agree on for a write neither person made.

### 7.7 What goes into `docs/sync-design.md`

Hello: "protocol 12: stashes". Migrations: `0021`. Shared tables: `stashes` and `stash_entries` with their references (account, stash,
transfer as sync ids). Section 11: "Stashes are never merged by name. A stash of an account deleted on the other device moves to the
main account; entries of a stash deleted on the other device are not taken; an entry whose transfer was deleted loses the link and
takes the later stamp. `stash-design.md`, section 7." The fixed order of 7.1.

## 8. Currency change

`src-tauri/src/currency.rs`, in `change` and `apply_recorded`, next to the other conversions:

- `convert_stashes`: for each stash **in an account of the app's currency** (`accounts.currency IS NULL`): `target_minor`
  (`at_least_one(convert(...))`) and every entry's `amount_minor` (`convert_signed`, at least one cent in size, sign kept). A stash in
  a foreign account is left alone: its amounts are in that account's currency.
- `Conversion` gains `stashes: u32` (stashes with a target or at least one entry converted) and `stash_entries: u32`.
- Triggers stand aside (`applying`), no stamp moves, the phone converts the same way. Saved after the change is a sum of converted
  pieces and may be a cent above the target: harmless (reached).
- The change dialog's answer and its toast list the counts as they list the others (UI: the place that shows `Conversion`).

## 9. Backup, restore, "Delete all my data"

- `backup.rs`: `TABLES` gains `"stashes"` and `"stash_entries"` after `"transfers"` (parents before children; the existing test fails
  until they are there).
- A file with an entry that points at a stash or a transfer it does not hold, or a stash at an account it does not hold, is refused
  whole by the foreign keys, in the one transaction.
- A backup from before `0021` restores and is migrated: no stashes.

## 10. Exports and the report

- **JSON** (`export.rs`): `Section` gains `Stashes`. With no limit: `stashes`, each `{ id, name, accountId, account, currency, target,
  targetDate, notes, archived, saved, entries: [{ id, kind, amount, date, note, transferId }] }` (entries oldest first), and every
  account gains `setAside` (its `free` is worked out from the balance and `setAside`, so it is not written). With a limit (dates,
  categories, ...) there is no `stashes` key at all (a file does not say what it was told to leave out), as transfers
  (`accounts-design.md` 10). `Preview` gains `stashes` and `stashEntries` counts, 0 when left out.
- **CSV**: no change (it is the transactions).
- **Excel** (`export/xlsx.rs`): sheets `Stashes` (Name, Account, Currency, Target, Target date, Saved, Archived, Notes) and
  `Stash entries` (Date, Stash, Kind, Amount, Note, Transfer: the other account's name or empty), not under a limit (so a workbook of
  stashes alone is refused under a limit, as one of transfers). Amounts of a foreign account in the plain number format, as section
  21.6 of `accounts-design.md`. Text cells that look like formulas are written as text, as everywhere.
- **PDF report** (`report.rs`, `report/layout.rs`): in a report of every category, when at least one stash exists, a short table
  "Stashes" after the accounts table: per stash (active by position, then archived): name, account, saved on the last day of the period,
  target (or "—"). New `ReportLabels` (`stashes`, `saved`, `target`), `serde(default)` English as the account labels
  (`accounts-design.md` 10), with their text in `ui/src/api/reportLabels.ts`. Foreign amounts carry their code ("RON 120.00").

## 11. Demo data

`src-tauri/seeds/dev.sql`, checked by `src-tauri/src/seed.rs` tests. Four stashes, each with entries spread over the months of the
seed (dates relative as the seed's other rows are):

- "Holiday in Lisbon", in Savings, target 1200.00, target date about seven months ahead, a monthly add of 100.00 (most of them with a
  transfer from the current account), one take of 40.00 with a note ("deposit for the flat").
- "New laptop", in the current account, target 900.00, no target date, reached (saved 900.00).
- "Rainy day", in Savings, no target, a few adds.
- "Bike", archived, target 450.00, reached and then taken out in full ("bought it").
- One in the lei account ("Wedding gift", RON, target 1000.00) with two adds.

Every add must respect the delta rule on the day the seed is loaded; the seed test checks `free >= 0` for every account and counts
5 stashes and at least 15 entries.

## 12. Command line and API

Routes in a new `src-tauri/src/api/stashes.rs` (tests `api/stashes_tests.rs`), listed in `ALL` in `api/routes.rs`. Route names have
two lowercase parts (`the_table_is_well_formed`).

| Route | Scope | Arguments | Command |
|---|---|---|---|
| `stashes.list` | read | `accountId` | `meowny stashes list` |
| `stashes.show` | read | `id` | `meowny stashes show 2` |
| `stashes.history` | read | `id`, `year` | `meowny stashes history 2 --year 2026` |
| `stashes.summary` | read | | `meowny stashes summary` |
| `stashes.create` | write | `name`, `accountId` (required), `target`, `targetDate`, `notes` | `meowny stashes create --name Holiday --account-id 2 --target 1200.00` |
| `stashes.update` | write | `id` and any of those (`target`, `targetDate`, `notes` nullable) | `meowny stashes update 2 --target 1500.00` |
| `stashes.archive`, `stashes.unarchive` | write | `id` | |
| `stashes.delete` | write | `id` (asks `--yes`, as the other deletes) | `meowny stashes delete 2 --yes` |
| `stashes.reorder` | write | `ids` | |
| `stashes.add` | write | `id`, `amount`, `date`, `note`, `fromAccountId` | `meowny stashes add 2 --amount 100.00 --note "november"` |
| `stashes.take` | write | `id`, `amount`, `date`, `note`, `toAccountId` | `meowny stashes take 2 --amount 40.00` |
| `stashes.amend` | write | `id` (the entry), `amount`, `date`, `note` (nullable) | `meowny stashes amend 7 --note "flat deposit"` |
| `stashes.erase` | write | `id` (the entry), `deleteTransfer` (boolean) — asks `--yes` like a delete (find how the confirm by verb works and make `erase` ask too) | `meowny stashes erase 7 --yes` |

- `amount`, `target` are `Kind::Money`. Summaries say plainly that money in a stash stays in its account, what free is, and every
  refusal (section 5) in a sentence, as the account routes do.
- After the routes: `cd cli && cargo run --example dump_schema > schema.json`; examples in the tour of `cli/tests/docs.rs` (create, add,
  add with a transfer, take, show, list, summary, one refusal: adding more than is free); `cd cli && UPDATE_DOCS=1 cargo test --offline
  --test docs`; `npm run docs`. The `meowny` skill is out of scope.

## 13. The screens

One page serves the computer and the phone; every part must be right at 360 px and at 1280 px, in light and dark. Reuse, do not
rebuild: `Table`, `Row`, `TapRow`, `IconAction` (`RowParts.tsx`), `useOpenRow`, `useReorder`, `Modal`, `ConfirmModal`, `Dropdown`,
`Pick`, `DatePicker`, `SectionTabs`, `EmptyState`, `Card`, `BalanceChart`, `YearNav`, `formatMoney` (and the foreign-currency money
formatting the account page uses), `useToast`, `errorText`, `saveChoice`/`storedChoice` (`lib/sticky.ts`), `useAccounts`,
`accountName`, `defaultAccountId` (`lib/accountsView.ts`). Lower-case words, the mono face, hairline boxes: the app's look.

### 13.1 Where it lives

- Route `stashes/:stashId?` in `App.tsx`, loader `stashes` in `pages/index.ts`, `StashesPage` lazy like the others.
- Sidebar (`AppSidebar.tsx`) right after accounts: label `nav.stashes`, icon `stash` (a new pixel jar in `NavIcon.tsx`, drawn in the same
  grid and stroke as the other icons), shortcut `goStashes` = `g` then `h` (`keys/shortcuts.ts`: `SHORTCUTS`, `GO_SECOND_KEY`, the
  shortcuts test).
- Phone: an icon in the top bar right after accounts. Check at 360 px that the bar still fits (`phone.css`); if it does not, shrink
  the gaps of `.phone-actions`, never hide the lock.

### 13.2 The list (`/app/stashes`)

- Title row: "stashes", the total saved (in the app's currency, from `Summary`), **new stash**.
- With more than one active account: an account `Pick` ("all accounts" first), remembered as `choice.stashes.account`, dropped when that
  account is gone.
- A grid of cards (one column on a phone, `auto-fill, minmax(18rem, 1fr)` on the computer), in the person's order, reorderable as
  accounts are (hold and drag, `Alt` + arrows) when no account filter is set. Each card:
  - the name; under it, small, the account's name (`accountName`, "(archived)" when the account is);
  - a bar (8 px, hairline track, accent fill; full and the success colour when reached; a stash without a target shows no bar but the
    saved amount large);
  - "450.00 of 1200.00 · 37%" (or "450.00 saved" without a target); saved below 0 in the danger colour;
  - one line of what is next: "750.00 left · 107.15 a month until june 2027", or "750.00 left" (no date), or "reached", or "overdue
    since 30 june 2026" (danger colour), or nothing;
  - two small buttons: **+ add** and **− take out**, opening `StashMoneyModal` on that tab (add hidden when the stash cannot take money:
    its account archived; take hidden when saved <= 0);
  - a click elsewhere on the card opens the stash.
- Under the grid: **archived (n)** switch (remembered, `choice.stashes.archived`), then the archived cards, greyed, with **restore** and
  **delete**.
- Empty: `EmptyState` with `stashes.emptyTitle`, `stashes.emptyText` (what a stash is in two sentences: money set aside in an account,
  the money stays there) and **new stash**.

### 13.3 One stash (`/app/stashes/:id`)

- **back**; the name; the account as a link to its page; **edit**, **archive** / **restore**, **delete** (the last two ask with
  `ConfirmModal`: archive says what it still holds stays set aside; delete says "the money stays in {account}; {amount} set aside is free
  again", and that linked transfers stay).
- A large progress block: the bar (12 px), saved and target in large numbers, percent, left, per month or overdue or reached, the
  target date; the notes under it (line breaks kept).
- Two main buttons: **add money** and **take out** (same rules as the card buttons).
- `BalanceChart` of `stash_history` with `YearNav`; when the stash has a target, a dashed line at the target (add an optional prop to
  `BalanceChart` if it has none; it must not change the account page).
- The history: a table (phone: `TapRow`s) newest first: date; a `+` or `−` mark in the success / danger colour; the note (or a dash);
  "from {account}" / "to {account}" as a link opening the transfer dialog (`TransferModal`) when the entry has a transfer; the amount
  (signed, coloured); saved after. A click on a row opens `StashMoneyModal` in edit mode.
- Free in the account, under the history: "{account}: {balance} · set aside {setAside} · free {free}"; free below 0 in the danger
  colour with `stashes.overBooked`.

### 13.4 `StashMoneyModal` (`components/StashMoneyModal.tsx`)

- Title "add to {name}" / "take out of {name}" / "edit entry". `SectionTabs` add / take out (not in edit mode).
- **amount**, with chips under it that fill it in one tap (only those that make sense and are above 0):
  - add: "the rest {left}" (with a target), "a month {perMonth}" (with per month), "all free {free}" (when free < left or no target);
  - take: "all {saved}", "half {saved / 2 rounded down to the cent}".
- **live preview**: a bar showing the stash before (solid) and after (striped segment, success colour for an add, danger for a take),
  and the line "after this: 550.00 of 1200.00 · 45%". When the amount breaks the delta rule, the line says why instead
  (`stashes.overFree` "only {free} of {account} is free", `stashes.overTarget` "that is {over} more than the target",
  `stashes.overSaved` "only {saved} is in the stash") in the danger colour, and **done** is disabled.
- **date** (`DatePicker`, today, no day after today), **note** (a two-line textarea, 200 characters, a small "n/200" counter).
- add: a switch **bring it from another account** with a `Dropdown` of the other active accounts in the same currency (hidden when there
  is none); with the switch on, the free check counts the transfer (the preview's free grows by the amount). take: **send it to
  another account**, the same. A line under the switch: "a transfer of {amount} from {from} to {to} is made too".
- Edit mode: amount, date, note; under them "came with a transfer from {account}" when linked, and "the transfer is not changed";
  **delete** at the left of the buttons, asking with `ConfirmModal`; when linked, the confirm has a checkbox "delete the transfer too".
- Keys: `Enter` submits, `Esc` closes, `Alt+1` / `Alt+2` choose the tab (reuse the entry-tab shortcuts if they fit, else none).

### 13.5 `StashModal` (`components/StashModal.tsx`)

Name; **account** (`Dropdown` of active accounts, always shown, even with one account; in edit only accounts in the same currency, and
a line "only accounts in {currency}" when that hides some); target (optional, "no target" when empty); target date (optional,
`DatePicker` with a clear button); notes (textarea, 1000). Section 5.3.

### 13.6 Elsewhere

- **Accounts list** (`AccountsPage`): under a balance, small, "set aside {setAside}" when it is not 0. **An account's page**: a part
  "stashes" under the breakdown with the account's stashes as small cards (name, bar, saved of target) linking to each, the line
  "set aside · free" from 13.3, and **new stash** (opens `StashModal` with this account).
- **Dashboard**: when `Summary.stashes > 0`, a card "stashes": the total saved and, under it, the first three active stashes in order
  with a small bar each; the card links to the stashes page.
- **Account delete**: `entries` now counts stashes, so the delete button is not offered for an account with one; the guide says why.
- **Sync review** (`ReviewModal.tsx`): the new field words. **Export page**: the stashes part in the list of parts.
- **Currency change** answer: the new counts where the others are listed.

### 13.7 API module

`ui/src/api/stashes.ts`: Zod schemas of section 6 (money `/^\d+\.\d{2}$/`, signed money `/^-?\d+\.\d{2}$/`, ids `/^[1-9]\d*$/`) and the
calls of 6.2. `api/accounts.ts`: `setAside`, `free`. `api/dataExport.ts`: the section and the counts. `lib/stashView.ts`: the pure
helpers the screens and tests share: `deltaCheck(before, change, free, target)` (section 3.1, returns `ok` or the reason with its
numbers), `chips(stash, kind)`, `progress(saved, target)`; all in minor units, no floating point on money.

## 14. Texts

New keys in `ui/src/i18n/en.ts` (lower case, as the others); the six other files (`de`, `es`, `fr`, `it`, `pt`, `ro`) get the same keys,
translated. In English the word is always "stash"; the other languages use their own everyday word for money put aside for a goal
(de "Spartopf", es "hucha", fr "cagnotte", it "salvadanaio", pt "mealheiro", ro "pușculiță"), the same word everywhere in a file.
Keys at least: `nav.stashes`, `stashes.title`, `stashes.add` (new stash), `stashes.addTitle`, `stashes.editTitle`, `stashes.account`,
`stashes.target`, `stashes.noTarget`, `stashes.targetDate`, `stashes.notes`, `stashes.saved`, `stashes.left`, `stashes.of` ("{saved} of
{target}"), `stashes.percent`, `stashes.perMonth` ("{amount} a month until {date}"), `stashes.reached`, `stashes.overdue`,
`stashes.addMoney`, `stashes.takeOut`, `stashes.addTitleMoney` ("add to {name}"), `stashes.takeTitle`, `stashes.editEntry`,
`stashes.chipRest`, `stashes.chipMonth`, `stashes.chipFree`, `stashes.chipAll`, `stashes.chipHalf`, `stashes.after`, `stashes.overFree`,
`stashes.overTarget`, `stashes.overSaved`, `stashes.bringFrom`, `stashes.sendTo`, `stashes.transferLine`, `stashes.cameWith`,
`stashes.transferKept`, `stashes.deleteTransferToo`, `stashes.fromAccount`, `stashes.toAccount`, `stashes.history`, `stashes.chart`,
`stashes.free`, `stashes.setAside`, `stashes.overBooked`, `stashes.archive`, `stashes.restore`, `stashes.archivedCount`,
`stashes.archiveText`, `stashes.deleteText`, `stashes.emptyTitle`, `stashes.emptyText`, `stashes.nameError`, `stashes.targetError`,
`stashes.reachedToast`, `stashes.addedToast`, `stashes.takenToast`, `stashes.sameCurrencyOnly`, `stashes.total`, `stashes.dashboard`,
`stashes.noteCount`, `sync.field.target`, `sync.field.targetDate`, `sync.field.stash`, `sync.field.kind`, `export.partStashes`,
`report.stashes`, `report.saved`, `report.target`, `keys.goTo` reused for `goStashes`. Check a key does not already exist before adding
it. The i18n test (every key in seven files, same placeholders) must pass.

## 15. Guide and docs

- `docs/site/guide.mjs`: a part `stashes` ("Stashes") after the accounts parts: what a stash is (money set aside in an account, it stays
  there), make one (an account is required), add and take out (notes, dates, bringing money from another account), what free means and
  the three limits, target and per month, archive and delete, an account with a stash cannot be deleted. **Keyboard shortcuts**: `g`
  then `h`. **On the phone**: the icon. **Export**: stashes are in the JSON and Excel files. **Privacy**, "What is stored": stashes
  (names, targets, notes) and their entries.
- `docs/site/shots.mjs` and `docs/site/images/`: `stashes` and `stash` (and `phone-stashes`), light and dark, if the shot tool runs here;
  otherwise list them as still to take in the final report.
- `README.md` "Budgeting" names stashes; the documentation table and `docs/site/develop.mjs` ("Design notes") list this file.
- `docs/accounts-design.md` section 1 "Not goals" and 20.1 "Not built": "piggy banks" now points to this file.
- This file: the status line says "built".

## 16. Choices, and why

1. **One account per stash, required.** The owner asked for it; it keeps free per account simple and the money's place clear.
2. **Money stays in the account; a stash only sets it aside.** A stash is not an account: no balance, net worth or budget changes.
3. **Entries, not a stored "current amount".** Saved is a sum, like a balance: no total to keep right on two devices, and a history
   with notes for free.
4. **The delta rule on local writes only.** A sync may join two fine writes into a stash above its target or below 0; refusing them
   would stop a sync for good (7.6).
5. **A transfer only when asked, and linked, not owned.** Adding money to a stash never moves money by itself; when the person asks, the
   transfer is an ordinary transfer that can be edited or deleted on its own, and deleting it never deletes the entry.
6. **The kind of an entry never changes.** An add edited to a negative number would be a take: deleting and making a new one is clearer.
7. **No future entries.** A stash's saved has no "on which day" question; the chart and the report still use the dates.
8. **Archived stashes still hold their money**, as an archived account keeps its balance; take it out first if it was spent.
9. **A stash's account changes only within one currency.** Amounts are in the account's currency; converting them on a move would need
   a rate history the app does not keep.
10. **No new error code.** `InvalidInput`, `NotFound`, `NameExists` cover every refusal; the screens explain before they would fail.

## 17. Open points

Money set aside automatically by a recurring transfer. A stash over several accounts. A stash in its own currency. Reordering stashes on
the phone. A "you are short" notice when free goes below 0.

## 18. Work packages

Package 1 first. Then 2, 3 and 5 can run together. 4 after 3 (the docs tour runs on the demo data). 6 last.

| # | Package | Files |
|---|---|---|
| 1 | Schema, Rust core, Tauri commands, their tests | `migrations/0021_stashes.sql`, `src/migrate.rs`, `src/stashes.rs`, `src/stashes/entries.rs`, `src/stashes/*_tests.rs`, `src/accounts.rs` (`entries`, `delete`, `setAside`, `free`), `src/commands.rs`, `src/lib.rs`, `build.rs`, `capabilities/*.json`, `src/schema_tests.rs` |
| 2 | Sync | `src/sync/wire.rs`, `src/sync/engine.rs`, `src/sync/stamp.rs`, `src/sync/*_tests.rs` (a new `sync/stashes_tests.rs`), `docs/sync-design.md` |
| 3 | Currency, backup, export, report, seed | `src/currency.rs`, `src/backup.rs`, `src/backup/stashes_tests.rs`, `src/export.rs`, `src/export/xlsx.rs`, `src/export/stashes_tests.rs`, `src/report.rs`, `src/report/layout.rs`, `seeds/dev.sql`, `src/seed.rs` |
| 4 | API routes, CLI, recorded docs | `src/api/stashes.rs`, `src/api/stashes_tests.rs`, `src/api/routes.rs`, `cli/schema.json`, `cli/tests/docs.rs`, `docs/site/commands.json`, `docs/site/examples.json` |
| 5 | UI and texts | section 13 and 14 files, `ui/src/styles/stashes.css` (imported where `accounts.css` is) |
| 6 | Guide and docs | section 15 |

Everyone runs `npm run check` before handing over (it runs the UI lint, build and tests, the docs check, the outbound check, both
`cargo test`s and both `clippy -D warnings`).

## 19. Tests

**Package 1**: the migration on an empty database and on one at version 20 with rows (nothing moved, no stamp, no history); every
`CHECK`, index and trigger of 4.2 to 4.4; stamps only on a real change; markers on delete and none for cascaded entries; the transfer
unlink; section 5.1 and 5.2 row by row with 5.4 items 1 to 17; the numbers of section 3 against a recomputation (percent, per month at
month ends and on the 31st, overdue, left, held, free with two stashes); `Account.setAside`/`free`/`entries`; the commands list test.

**Package 2**: wire refusals for every field; two devices: a first copy, random edits end equal (fixed seeds), 7.6 (both add, both take,
target exceeded), a stash deleted on one while the other adds (entries dropped, stash gone on both), a stash edited after the other
deleted it (back on both, its entries sent again), an account deleted while the other adds a stash to it (stash in the main account
with equal stamps), a linked transfer deleted on one (link cleared on both, equal stamps), a conflict listing the new facts, a protocol
11 peer, a sync cut at every message.

**Package 3**: currency (a main-currency stash converted, a foreign one not, targets and signed entries at the bounds, tiny rates keep
one cent, `apply_recorded` equal); backup round trip and the tables list; JSON, Excel with and without a limit; formula-looking names;
the report table; the seed (5.4 rules hold: `free >= 0`, counts).

**Package 4**: each route's scope, arguments, answers and refusals as status and code; `schema.json` and the recorded docs up to date.

**Package 5** (`*.test.ts`): `stashView` (`deltaCheck` at every bound of 3.1, `chips`, `progress`), the Zod schemas, the i18n test, the
shortcuts test; by hand at 360 px and 1280 px, light and dark: 5.3 for both dialogs.

**Package 6**: `npm run check:docs`.
