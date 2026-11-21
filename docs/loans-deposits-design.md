# Loans, deposits and return rates design

Credits, loans and mortgages as accounts you owe on; term deposits as stashes with a return rate; a return rate on savings accounts.
Section numbers are cited in the code; keep them. Builds on `accounts-design.md` and `stash-design.md`; where this file disagrees with
them, this file wins. Migration `0022_loans_deposits.sql`. Sync protocol **13**. Status: being built.

## 1. What it has to do

1. **The stashes page is a list**, not a grid of cards. Each row has a **+** and a **−** button that open the add or take dialog of
   that stash. The stash's own page stays as it is.
2. **Three liability kinds of account**: `CREDIT`, `LOAN`, `MORTGAGE`. Their balance is what you owe (below 0). You pay them with a
   **split payment**: the interest part is an expense from the paying account, the rest is a transfer that lowers the debt.
3. **A rate** (percent, per month or per year) on any account: a return on a savings account, the interest of a liability. The app
   shows estimates and, for an asset account, a **book interest** button that opens the income dialog filled in.
4. **The debts page has two tabs**: **friends** (what it is today) and **debts** (the liability accounts, with pay and new).
5. **Term deposits**: a stash of kind `DEPOSIT` with a rate, a start date and a maturity date. States running, matured, booked,
   broken (section 5.3). A **book return** at maturity makes the income and the add in one step.
6. Everything follows sync, currency change, backup, exports, the report, the demo data, the CLI and the local API. No network, no new
   dependency, no automatic booking of anything.

Not goals: interest booked by itself; daily rates; compound interest in the estimates (they are simple interest, said in the guide);
liabilities someone owes you (friends do that); recurring payments of a loan (a recurring transfer does not exist); amortisation tables.

UI rules for this work: reuse components (`Table`/`Row` of `RowParts.tsx`, `SectionTabs`, `Modal`, `Pick`, `AccountField`,
`EntryDialog`, `StashMoneyModal`, `AccountModal`); list views over cards; no help paragraphs and no `title`/tooltip text on buttons
whose text says what they do. Icon-only buttons keep their `aria-label` (accessibility is not cut).

## 2. The pieces

| Name | Where |
|---|---|
| Account kinds `CREDIT`, `LOAN`, `MORTGAGE` (the **liabilities**); the others are **assets** | `accounts.kind` |
| Rate of an account | `accounts.rate_milli`, `accounts.rate_period` |
| Installment of a liability | `accounts.installment_minor` |
| Split payment | `accounts::pay` in `src-tauri/src/accounts/pay.rs` |
| Stash kind `GOAL` (every stash today) or `DEPOSIT` | `stashes.kind` |
| Rate, start, broken day, return booked day of a deposit | `stashes.rate_milli`, `rate_period`, `start_date`, `broken_on`, `returned_on` |
| Maturity of a deposit | `stashes.target_date` (the column a goal uses for its target date) |
| Book return | `stashes::book_return` in `src-tauri/src/stashes/deposits.rs` |
| Rate maths (shared by both) | `src-tauri/src/rates.rs` |

## 3. The numbers

Integers in minor units, `i128` for products. **Rounding** of every estimate: half away from zero to the minor unit.

**A rate** is typed as a percent with at most 3 decimals, "0" to "100" ("3.25", "4.125", "0.5"). Stored as `rate_milli` = percent ×
1000 (3.25 % = 3250), 0 to 100000. On the wire and in the API it is the decimal string as `format_rate` writes it: no trailing zeros
after the point, no point when whole ("3.25", "4.125", "4", "0"). Period `MONTHLY` or `YEARLY`. A rate and its period are both set or
both absent.

**Monthly rate** `m` of a rate: `YEARLY` → `rate_milli / 100000 / 12`; `MONTHLY` → `rate_milli / 100000`.
`interest(amount)` for one month = `round(amount × rate_milli / 100000 / 12)` (yearly) or `round(amount × rate_milli / 100000)`
(monthly), worked out as one `i128` division with rounding.

### 3.1 Asset accounts (kinds `CHECKING`, `SAVINGS`, `CASH`, `CREDIT_CARD`)

- **return per month** = `interest(max(balance, 0))`; none without a rate. **return per year** = return per month × 12.

### 3.2 Liabilities

- **owed** = `max(−balance, 0)`. The opening balance of a new liability is what you owed then, stored negative.
- **State** (worked out, stored nowhere): `OWING` (balance < 0), `PAID_OFF` (balance = 0), `OVERPAID` (balance > 0).
- **interest per month** = `interest(owed)`; none without a rate.
- **payoff** (needs an installment `I`; none without one or when owed is 0): start with `b = owed`, then month by month:
  `i = interest(b)`; when `I <= i` there is **no payoff** (the debt never falls: `payoff = never`); else `b = b + i − I`, add `i` to
  `interest_left` and count the month, until `b <= 0`. Stop after 1200 months: then `never` too. Answers `months`, `interest_left`
  (the sum of the `i`), and `month` = the month of today plus `months` as `YYYY-MM`. Rate 0 or none: `months = ceil(owed / I)`,
  `interest_left = 0`.

### 3.3 Deposits

`days(a, b)` = calendar days from date a to date b (0 when b ≤ a).

- **term** = `days(start_date, maturity)`.
- **return** (expected for the whole term) = `round(saved × rate_milli × term / (100000 × 365))` for `YEARLY`, and
  `round(saved × rate_milli × term × 12 / (100000 × 365))` for `MONTHLY`. Uses `held` (`max(saved, 0)`), not `saved`.
- **value at maturity** = held + return.
- **days left** = `days(today, maturity)`.

## 4. The rows (migration `0022_loans_deposits.sql`)

### 4.1 `accounts` is rebuilt

The kind `CHECK` cannot be widened in place (`accounts-design.md` 20.1), so the table is rebuilt by SQLite's 12-step way, foreign keys
being off while a migration runs (`migrate.rs`; checked before the commit):

1. Drop every view that reads `accounts` (`account_movements`, and any other found with
   `SELECT name FROM sqlite_master WHERE type = 'view'`) and every trigger `ON accounts` (their latest bodies are in 0018, 0019, 0020,
   0021: `_updated_at`, `_sync_insert`, `_sync_update`, `_sync_delete`, `_audit_insert/_update/_delete`, the main-account guard, the
   currency guard, any other).
2. `CREATE TABLE accounts_new` with every column of today's table, same order, same `CHECK`s, then the new ones:
   - `kind` `CHECK (kind IN ('CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD', 'CREDIT', 'LOAN', 'MORTGAGE'))`
   - `rate_milli INTEGER CONSTRAINT ck_accounts_rate_milli CHECK (rate_milli IS NULL OR rate_milli BETWEEN 0 AND 100000)`
   - `rate_period TEXT CONSTRAINT ck_accounts_rate_period CHECK ((rate_milli IS NULL) = (rate_period IS NULL) AND (rate_period IS NULL OR rate_period IN ('MONTHLY', 'YEARLY')))`
   - `installment_minor INTEGER CONSTRAINT ck_accounts_installment CHECK (installment_minor IS NULL OR (installment_minor BETWEEN 1 AND 999999999999 AND kind IN ('CREDIT', 'LOAN', 'MORTGAGE')))`
3. `INSERT INTO accounts_new (…every old column…) SELECT … FROM accounts`; `DROP TABLE accounts`;
   `ALTER TABLE accounts_new RENAME TO accounts`.
4. Make again every index, view and trigger of step 1, word for word as their latest version, with the three new columns added to every
   column list (`UPDATE OF` lists, the `WHEN … IS NOT OLD …` tests, the `json_object` of the three audit triggers).
5. The main account's row is unchanged (the new columns are `NULL`), so no device stamps it.

`sqlite_sequence` is not used (`INTEGER PRIMARY KEY` without `AUTOINCREMENT`); keep ids as they are. A test proves: every account, its
id, sync id and stamp survive; the foreign keys of `transactions`, `recurring_transactions`, `ledger_entries`, `transfers`, `stashes`
still hold (`PRAGMA foreign_key_check` empty); `sqlite_master` lists the same triggers and views by name before and after.

### 4.2 `stashes` gains columns

`ALTER TABLE stashes ADD COLUMN`, in this order (a column `CHECK` may name a column added before it):

| Column | Rule |
|---|---|
| `kind TEXT NOT NULL DEFAULT 'GOAL'` | `CHECK (kind IN ('GOAL', 'DEPOSIT'))` |
| `rate_milli INTEGER` | `NULL` or 0 to 100000; `CHECK ((kind = 'DEPOSIT') = (rate_milli IS NOT NULL))` |
| `rate_period TEXT` | `CHECK ((rate_milli IS NULL) = (rate_period IS NULL) AND (rate_period IS NULL OR rate_period IN ('MONTHLY', 'YEARLY')))` |
| `start_date TEXT` | a real date; `CHECK ((kind = 'DEPOSIT') = (start_date IS NOT NULL))` |
| `broken_on TEXT` | `NULL` or a real date; only with `kind = 'DEPOSIT'` |
| `returned_on TEXT` | `NULL` or a real date; only with `kind = 'DEPOSIT'` |

A deposit also needs `target_date IS NOT NULL AND target_date > start_date`: that rule lives in the core (`ALTER TABLE` cannot add a
table check, and `target_date` came before). The stash triggers that list columns (`trg_stashes_updated_at`, `_sync_update`, the three
audit triggers) are dropped and made again with the six new columns, as 0019 did for accounts.

## 5. State machines

Refusals answer existing `AppError` values only (`InvalidInput`, `NotFound`, `NameExists`). A refused event writes nothing and makes no
stamp. "Liability" = kind `CREDIT`, `LOAN`, `MORTGAGE`; "asset" = the other four.

### 5.1 An account's kind and rate (adds to `accounts-design.md` 5.1)

| State | Event | Guard | Next | Effect, or refusal |
|---|---|---|---|---|
| `NONE` | create | as today; `rate` and `ratePeriod` both given or both absent; rate valid (section 3); `installment` only for a liability, "0.01" to "9999999999.99" | `ACTIVE` | else `InvalidInput` |
| `ACTIVE` | change kind inside its group (asset ↔ asset, liability ↔ liability) | | `ACTIVE` | written |
| `ACTIVE` | change kind across groups | | `ACTIVE` | `InvalidInput`. So the main account (an asset) is never a liability, and no stash is ever on a liability |
| `ACTIVE` | set, change or clear the rate (`rate: null` clears both) | rate valid; a period with the rate | `ACTIVE` | written |
| `ACTIVE` | set, change or clear the installment (`installment: null` clears) | the account is a liability | `ACTIVE` | else `InvalidInput` |
| `ARCHIVED` | any of the above | | | `InvalidInput`, as every update of an archived account today |
| any | currency conversion (`currency.rs`) | | | `installment_minor` converted with the opening balance; the rate is a percent and is not touched |

### 5.2 A liability (worked out from the balance)

| State | Event | Guard | Next | Effect, or refusal |
|---|---|---|---|---|
| `OWING` | pay (5.4) with principal p | | `OWING` / `PAID_OFF` / `OVERPAID` by the new balance | a transfer (and an expense) |
| `PAID_OFF` | pay | | `OVERPAID` | allowed; the screens show the state |
| any | archive / restore / delete | as any account | | as today; a liability with rows is not deleted |
| any | a stash is made in it or moved to it | | | `InvalidInput` (`stashes::create`, the move of `stashes::update`) |

### 5.3 A deposit stash (adds to `stash-design.md` 5.1 and 5.2; a `GOAL` stash is unchanged)

States, worked out from the row and today: `RUNNING` (`broken_on` null, today < maturity), `MATURED` (`broken_on` null,
`returned_on` null, today ≥ maturity), `BOOKED` (`broken_on` null, `returned_on` set, today ≥ maturity), `BROKEN` (`broken_on` set).
`archived` stays the flag it is today: closing a deposit is archiving it.

| State | Event | Guard | Next | Effect, or refusal |
|---|---|---|---|---|
| `NONE` | create `kind: DEPOSIT` | the stash rules; `rate` + `ratePeriod` required; `startDate` a real date (left out: today); `targetDate` (maturity) required and `> startDate` | `RUNNING` or `MATURED` by today | else `InvalidInput` |
| any | update `kind` | | | `InvalidInput`: the kind is fixed when the stash is made. A `GOAL` patch with deposit fields is `InvalidInput` |
| `RUNNING` | add | the stash rules (delta rule, active, account active) | `RUNNING` | entry |
| `MATURED`, `BOOKED`, `BROKEN` | add | | | `InvalidInput` |
| `RUNNING` | take of exactly `saved` (with or without `toAccountId`) | the stash rules | `BROKEN` | entry; `broken_on` = the entry's date |
| `RUNNING` | take of less than `saved` | | | `InvalidInput` |
| `MATURED`, `BOOKED` | take | the stash rules | same | entry. Taking all leaves the state; the screens then offer archive |
| `BROKEN` | take | the stash rules | `BROKEN` | entry (a sync may have left money in it) |
| `MATURED` | book return (5.5) | | `BOOKED` | income + add + `returned_on` |
| `RUNNING`, `BOOKED`, `BROKEN` | book return | | | `InvalidInput` |
| `RUNNING`, `MATURED`, `BOOKED` | update rate, period, start date, maturity | maturity `> start`; rate valid | by today | written. **Roll over**: from `MATURED` or `BOOKED`, a new maturity after today sets `returned_on` = null and, without a `startDate` in the patch, `start_date` = the old maturity, so the next term starts where the last one ended (`RUNNING`). A deposit is never `overdue` |
| `BROKEN` | update rate, period, start date, maturity | | | `InvalidInput`. Name, notes, target: allowed |
| `RUNNING`, `MATURED`, `BOOKED` | edit an entry's amount or date, delete an entry | the stash rules, and the add/take rules of this table for what the edit does (an edit that puts money in when not `RUNNING` is refused; an edit that takes money out while `RUNNING` must leave `saved = 0`, then `BROKEN`) | | |
| `BROKEN` | edit an entry's amount or date, delete an entry | | | `InvalidInput`; only the note of an entry can change. To undo a break, delete the stash |
| any | archive, restore, delete, move, reorder | as a goal stash | | as today |
| sync | rows | | | no check (`stash-design.md` 7.6): `broken_on` and `returned_on` are columns of the row and travel with it |

### 5.4 Pay a liability (`accounts::pay`)

Input: `liabilityId`, `fromAccountId`, `amount`, `interest` (left out: "0.00"), `categoryId` (needed when interest > 0), `date` (left
out: today), `note` (optional). One database transaction; any refusal writes nothing.

| Guard | Refusal |
|---|---|
| the liability exists, is a liability, is `ACTIVE` | `NotFound` for an unknown id, else `InvalidInput` |
| `fromAccountId` exists, is `ACTIVE`, is not the liability, has the liability's currency | `InvalidInput` |
| amount "0.01" to "9999999999.99"; interest "0.00" to amount | `InvalidInput` |
| interest > 0: `categoryId` is an expense category; the expense obeys every rule `transactions::create` has for that account (a foreign account: the core passes what the entry dialog passes for it) | `InvalidInput` |
| date, note: the rules of a transfer | `InvalidInput` |

Effect: when `principal = amount − interest > 0`, a transfer from → liability of `principal` (date, note) through
`accounts::transfers::create`; when `interest > 0`, an expense in the from account of `interest`, name = the liability's name, the
category, date, note, through `transactions::create`. Answers `{ transfer: Transfer | null, transaction: Transaction | null }`. The two
rows are ordinary rows afterwards: editing or deleting one does not touch the other.

### 5.5 Book a deposit's return (`stashes::book_return`)

Input: `stashId`, `amount`, `categoryId`, `date` (left out: today), `note` (optional). Guards: the stash is a `DEPOSIT` in state
`MATURED`, not archived, its account `ACTIVE`; amount "0.01" to "9999999999.99"; `categoryId` is an income category; the income obeys
`transactions::create`'s rules for that account; date not after today. Effect in one database transaction: an income transaction in the
stash's account (name = the stash's name, the amount, category, date, note), then an add entry of the amount (same date and note; the
delta rule sees the income), then `returned_on` = date. Answers the stash.

### 5.6 The dialogs

Every new or changed dialog follows `stash-design.md` 5.3: `CLOSED → EDITING → SAVING → CLOSED`, errors back to `EDITING` with a toast,
done disabled while invalid, a second submit ignored while `SAVING`.

- **Pay** (`LoanPayModal`): opens with from = the last account used to pay (sticky, `saveChoice`), else the main account; amount =
  the installment, else empty; interest = the interest per month estimate, capped at the amount, else "0.00"; category = the last one
  used (sticky), else none; date today. Live: principal = amount − interest, and owed after. Done disabled while amount is empty or
  wrong, interest > amount, or interest > 0 without a category.
- **Add / take** (`StashMoneyModal`) on a deposit: the add tab only while `RUNNING`; the take tab while `RUNNING` is a **break**: the
  amount is fixed to saved and not editable, done says "break deposit". A third mode **return** (`MATURED` only): amount = the return
  estimate, an income category (sticky), date, note; no transfer; done calls `book_return`.
- **Stash** (`StashModal`): when making, a two-tab switch **goal | deposit**; the deposit tab shows rate, period, start date and
  maturity instead of target date. When editing, no switch; a `BROKEN` deposit shows its rate fields read-only.
- **Account** (`AccountModal`): opened from the debts tab it offers only the liability kinds and labels the opening balance "owed"
  (typed positive, sent negative); from the accounts page only the asset kinds. It shows rate + period for every kind, installment for
  liabilities only.
- **Book interest** of an asset account: `EntryDialog` on the income tab with `initial` = `{ tab: 'income', accountId, amount: return
  per month, note: <the interest text> }`. `EntryInitial` gains `amount` and `note`.

## 6. Rust core

- `src-tauri/src/rates.rs`: `parse_rate(&str) -> Result<i64>` (milli), `format_rate(i64) -> String`, `Period` enum
  (`MONTHLY`/`YEARLY`, serde `SCREAMING_SNAKE_CASE`), `month_interest(amount, milli, period) -> i64`, `payoff(owed, installment, milli,
  period, today) -> Option<Payoff>` (`None` = no installment or owed 0; `Some(Payoff::Never)` / `Some(Payoff::Months { months,
  interest_left, month })`), `deposit_return(held, milli, period, start, maturity) -> i64`. Unit tests here cover the edge cases of
  section 9.
- `accounts.rs`: `Kind` gains `Credit`, `Loan`, `Mortgage` and `fn is_liability(self)`. `Account` gains `rate: Option<String>` — the
  name is taken by the currency rate, so the new fields are `interest_rate: Option<String>`, `interest_period: Option<Period>`,
  `installment: Option<String>`, and the worked-out `liability: Option<Liability>` (liabilities only: `owed`, `state`,
  `interest_per_month`, `payoff`) and `return_per_month: Option<String>` (assets with a rate). JSON: `interestRate`,
  `interestPeriod`, `installment`, `liability`, `returnPerMonth`. `NewAccount` and `AccountPatch` gain `interest_rate`,
  `interest_period`, `installment` (patch: `present_or_null` for `interest_rate` and `installment`; clearing the rate clears its
  period).
- `Summary` gains `debts`: the sum of the liabilities' balances that count in net worth, in the app's currency (signed, usually below
  0). `in_accounts` becomes the assets only. `net_worth = in_accounts + debts + friends`. Every reader of `inAccounts` (dashboard,
  accounts page, report, CLI) is checked.
- `accounts/pay.rs`: `Payment` input and `pay(conn, &Payment, today) -> Result<Paid>` (5.4). Tauri command `accounts_pay`.
- `stashes.rs`: `Stash` gains `kind` (`GOAL`/`DEPOSIT`) and `deposit: Option<Deposit>` = `{ interestRate, interestPeriod,
  startDate, brokenOn, returnedOn, state, expectedReturn, valueAtMaturity, daysLeft }` (maturity is the stash's `targetDate`).
  `NewStash` gains `kind`, `interestRate`, `interestPeriod`, `startDate`; `StashPatch` gains `interestRate`, `interestPeriod`,
  `startDate` (and refuses `kind`). Goal stashes keep every number they have; for a deposit `perMonth` is none.
- `stashes/entries.rs`: the guards of 5.3 in add, take, edit, delete.
- `stashes/deposits.rs`: `book_return` (5.5). Tauri command `stashes_book_return`.
- `stashes::create` and the move refuse a liability account.

## 7. Sync (protocol 13)

`wire::PROTOCOL` = 13. The account row gains `interestRate` (milli, integer or null), `interestPeriod`, `installment` (minor or null);
the stash row gains `kind`, `interestRate`, `interestPeriod`, `startDate`, `brokenOn`, `returnedOn`. A row from a peer with a kind or
value the `CHECK`s refuse is refused as any bad row is today. Nothing else changes: no new table, no new marker kind. A device on
protocol 12 is told to update, as for every bump. `docs/sync-design.md` gets the new protocol line and the new fields.

## 8. Currency, backup, exports, report, demo data

- **Currency change**: `installment_minor` of main-currency accounts converted like `opening_balance_minor`; counted in `Conversion`.
- **Backup / restore / delete all**: the new columns go with their tables; a test restores a backup with a mortgage and a deposit.
- **JSON / Excel export**: accounts gain `interestRate`, `interestPeriod`, `installment`; stashes gain `kind`, `interestRate`,
  `interestPeriod`, `startDate`, `brokenOn`, `returnedOn` (Excel: the same columns on the existing sheets).
- **Report**: the accounts table names the new kinds; the net worth line is assets + debts + friends, with a "debts" line.
- **Demo data** (`seed.rs`): the savings account gets 2.5 % yearly; a `MORTGAGE` "Flat" (owed 180000.00 since two years ago, 3.9 %
  yearly, installment 950.00) and a `LOAN` "Car" (owed 9000.00 since a year ago, 6.5 % yearly, installment 300.00), each with the
  split payments of the last three months from the current account (category "Interest", made if missing, or the closest existing
  expense category); one deposit stash "Term deposit" in the savings account, 3.1 % yearly, 12 months, `RUNNING`, with one add.

## 9. Tests: edge cases only

No test that repeats what an existing test proves for goal stashes or plain accounts.

1. Migration: rebuild keeps rows, ids, stamps, foreign keys, triggers and views (4.1); an account update after it still stamps and
   audits.
2. `parse_rate`: "0", "100", "100.001" (refused), "3.1415" (refused, 4 decimals), "-1", "", "abc", "3." and ".5" (follow
   `parse_amount`'s rules for those).
3. `month_interest` rounding at half a cent, both periods; rate 0.
4. `payoff`: installment equal to the first month's interest (`never`); one cent above (finishes, < 1200 months?: assert what it gives
   or `never`); rate 0; owed less than one installment (1 month); owed 0 (none).
5. Kind change across groups refused; inside a group allowed; installment on an asset refused; clearing the rate clears the period.
6. Stash create in a liability refused; move to a liability refused.
7. Pay: interest = amount (expense only, no transfer); interest 0 (transfer only); interest > amount refused; interest > 0 without a
   category refused, with an income category refused; from = the liability refused; another currency refused; a refusal leaves no
   transfer and no transaction; overpay allowed (`OVERPAID`).
8. Deposit: add after maturity refused; partial take while running refused; whole take while running → `BROKEN` with `broken_on`;
   add/edit/delete in `BROKEN` refused, note edit allowed; book return in `RUNNING`, `BOOKED`, `BROKEN` refused; book return in
   `MATURED` writes income + add + `returned_on`; roll over from `BOOKED` sets start = old maturity and clears `returned_on`; maturity
   ≤ start refused; kind change refused.
9. Summary: a mortgage lowers net worth through `debts`, not `in_accounts`; an excluded liability counts nowhere.
10. Sync: a mortgage and a deposit with `broken_on` travel both ways; protocol 13.
11. UI (vitest, pure functions only): the pay dialog's live numbers and done guard; the deposit modes of the money dialog (break amount
    fixed, return only when matured).

## 10. Screens

- **Stashes list** (`/app/stashes`): `Table` with columns name (link to the stash page), account, saved (with a thin `StashBar`),
  target or maturity, and an actions cell with **+** and **−** (`Button` size `sm`, text "+" and "−", `aria-label` "add money to X" /
  "take money from X"). For a deposit the row shows its state word (running, matured, booked, broken) and, when `MATURED`, a
  **return** button in place of +. + is hidden when an add is not allowed (`canAdd`), − when a take is not (`canTake`). Reorder by drag
  keeps working on the rows. The archived list under the toggle uses the same table. The phone uses `TapRow` as the other lists do.
- **Stash page**: a deposit shows rate, start, maturity, days left, expected return, value at maturity, state; its buttons follow 5.3.
- **Debts page**: `SectionTabs` **friends | debts** at the top, kept in the address as `?tab=debts`. The friends tab is today's page.
  The debts tab: total owed, a **new** button (`AccountModal` with liability kinds), and a `Table` of the liabilities (archived ones
  last, dimmed): name (link to `/app/accounts/:id`), kind, owed, rate, installment, payoff (`MMM YYYY`, or "never"), and a **pay**
  button (`LoanPayModal`). Empty: one line, "no debts".
- **Accounts page**: liabilities are not listed (they are on the debts tab); the totals show assets. An asset with a rate shows "≈ X
  / month" in its row. The account's page (`/app/accounts/:id`) works for liabilities too: there it shows owed, rate, installment,
  payoff and a **pay** button instead of the add buttons; for an asset with a rate a **book interest** button.
- **Pickers**: `AccountField` (expenses, income, recurring, friends' entries, stashes) lists assets only. `TransferModal` and the pay
  dialog's from field list every active account (pay: assets only).
- **Dashboard**: net worth as the summary gives it; a "debts" figure beside it when there is any liability.

## 11. Texts

Every new string in all 7 languages (`ui/src/i18n/*.ts`), keys under `accounts.kind.CREDIT|LOAN|MORTGAGE`, `accounts.rate*`,
`accounts.installment`, `accounts.returnPerMonth`, `accounts.bookInterest`, `debts.tabFriends`, `debts.tabDebts`, `loans.*` (owed,
pay, principal, interest, payoff, never, states, new, none), `stashes.deposit*` (kind switch, rate, start, maturity, states, break,
return, expected, value at maturity, days left). The report labels gain the new kinds and "debts".

## 12. Command line and API

| API / CLI | What |
|---|---|
| `accounts.create` / `accounts.update` | gain `interestRate`, `interestPeriod`, `installment`; kinds `CREDIT`, `LOAN`, `MORTGAGE` |
| `accounts.pay` (`POST /accounts/:id/pay`) | 5.4. CLI `meowny accounts pay <loan> --from <account> --amount 950 --interest 585.00 --category Interest [--date] [--note]` |
| `stashes.create` / `stashes.update` | gain `kind`, `interestRate`, `interestPeriod`, `startDate` |
| `stashes.return` (`POST /stashes/:id/return`) | 5.5. CLI `meowny stashes return <stash> --amount 31.00 --category Interest [--date] [--note]` |

`cli/schema.json` and the recorded docs follow; `accounts list` prints owed/payoff for liabilities.

## 13. Work packages

1. **Core accounts**: migration 0022 (all of section 4), `rates.rs`, kinds, rate, installment, liability numbers, `Summary.debts`,
   `pay`, the stash refusal of liabilities, Tauri commands, tests 1–7 and 9.
2. **Core deposits**: the deposit columns in `stashes.rs`, 5.3 in `entries.rs`, `deposits.rs`, Tauri command, test 8.
3. **Sync, currency, backup, exports, report, demo data**: sections 7 and 8, test 10.
4. **CLI and API**: section 12.
5. **UI**: sections 5.6, 10, 11, test 11.
6. **Docs**: the guide (debts tab, deposits, rates), the landing page line, README, screenshots, this file marked built.
