# Expense Tracker: Build Plan

Companion to `expense_tracker_schema.txt` (schema v3). Work top to bottom. Each phase ends with a "Done when" check, so you know when to move on.

**Size tags:** S = a few hours, M = a few days, L = a week or more (rough, for one person working part-time).

## Order and why

1. **Foundations and privacy basics first**, because auth tables, ids and erasure are painful to retrofit.
2. **Core budgeting loop next**: categories, transactions, budgets. This is the product.
3. **Recurring, then charts**: both build on the core.
4. **GDPR completion and deployment before inviting anyone else.** Phase 7 is a gate.
5. **Features that need other people or outside parties last**: admin board, expense splitting, bank connection (riskiest last).
6. **Native app last**, because the mobile web version covers most of the need.

Phases 10 and 11 can swap places if the bank feature matters more to you than splitting. Phase 12 can move earlier if the PWA is not enough.

## Decisions already made

- Backend: Node.js + Express, PostgreSQL 14+.
- Auth: Better Auth (email + password, Google sign-in).
- Users have uuid ids; other tables use bigint ids.
- Budgets are effective-dated; recurring items use categories; history via database triggers.
- Money is handled as strings/decimals in code, never floats.
- Query layer: Kysely. Schema stays in SQL migrations.
- Migrations: node-pg-migrate, SQL files.
- Later features (not modeled yet): expense splitting, admin board, bank connection, mobile app.

## Open decisions (decide when you reach the phase)

| Decision | Needed by | Notes |
|---|---|---|
| Email sender (SMTP or a provider) | Phase 2 | Needed for verification and password reset; it is a data processor |
| Hosting and backups | Phase 7 | If self-hosting: disk encryption, encrypted backups, TLS |
| Chart library | Phase 5 | Any that works with your frontend |
| History retention cap | Phase 6 | Keep forever while the account exists, or cap (e.g. 3 years)? |
| Erasure grace period | Phase 6 | Suggested 7 days; must stay well under one month |
| Bank aggregator | Phase 11 | Do a spike first (see phase) |

---

## Phase 0: Setup and decisions (S)

- [x] Repo layout: `server/` is the API. The React app stays in `client/` until a frontend phase. No `shared/` package yet.
- [x] TypeScript, Express 5, oxlint, `.env` handling (secrets never in git). No separate formatter.
- [x] PostgreSQL in Docker for dev, plus a separate throwaway database for tests
- [x] Pick migration tool and query layer (see decisions above)
- [x] Request validation with Zod at the API boundary
- [x] Logging that never prints request bodies or personal data
- [x] Test setup: unit tests plus integration tests against a real Postgres (Docker)
- [x] CI that runs lint, tests and migrations from scratch
- [x] Start a one-page **data inventory** (what personal data, why, where, how long, which processors). Update it every phase.

**Done when:** an empty Express app starts, connects to Postgres, one migration runs from scratch in CI, and one test passes.

## Phase 1: Database foundation (M)

- [x] Migration 1: all tables, indexes and foreign keys from Part 1 of the schema file (`002_tables.sql`. `001_init.sql` only enables `pgcrypto`)
- [x] Migration 2: CHECK constraints, partial unique indexes, triggers (Part 2, `003_constraints.sql`)
- [x] Migration 3: statistics functions (Part 3) and GDPR functions (Part 4, `004_functions.sql`)
- [x] **Reconcile Better Auth** with better-auth 1.7.7: uuid ids, snake_case tables and fields. The published CLI package is deprecated, so this used that version's own schema compiler. Auth index names follow it, and the schema file was updated.
- [x] Constraint tests (these are your safety net):
  - [x] a transaction cannot use another user's category
  - [x] a budget's type must match its category's type
  - [x] only one GLOBAL budget per user, type and month
  - [x] a category type cannot change after creation
  - [x] the same recurring occurrence cannot be paid twice
  - [x] the same `client_uuid` cannot create two transactions
  - [x] audit rows appear for insert, update and delete, and are skipped when only `updated_at` changed
- [x] Dev seed script with a demo user, categories, budgets and transactions

**Done when:** a fresh database builds from migrations only, Better Auth's generated schema matches yours, and all constraint tests pass.

## Phase 2: Auth, onboarding and account page (M)

- [x] Mount Better Auth in Express. Follow its docs for Express: its handler must come **before** the JSON body parser.
- [ ] Email + password sign-up with email verification and password reset (needs the email sender)
- [ ] Google sign-in: create the OAuth client in Google Cloud Console, set redirect URIs for dev and prod, keep automatic account linking strict (verified emails only)
- [x] Privacy settings in Better Auth config: turn off IP tracking if possible, do not keep provider tokens if you can skip them, short session lifetime, secure cookies, trusted origins/CORS for your frontend only
- [x] Rate limiting on auth endpoints
- [ ] Onboarding screen: accept Terms + Privacy Policy (`user_consents`) and choose currency (`user_settings`) in one request. Users without a `user_settings` row are sent here.
- [x] Auth middleware: load the session, put `userId` on the request, reject everything else
- [ ] Account page: edit name, change currency/locale/timezone, change password, linked logins, list and revoke sessions, **request account deletion** (sets `deletion_requested_at`, signs the user out)
- [ ] First draft of Terms and Privacy Policy text (versioned, e.g. `2026-10-01`)
- [ ] Tests: sign-up, verification, login, Google (mocked), reset, session revoke, unauthenticated access is rejected

**Done when:** a new user can sign up by email or Google, finish onboarding, log in and out, reset a password, and request deletion.

## Phase 3: Core budgeting: categories, transactions, budgets (L)

- [ ] Every query is scoped by `user_id` from the session, never from the request body
- [ ] Category groups and categories CRUD. Soft delete writes the NULL budget row (schema Part 5-A).
- [ ] Transactions CRUD with filters (date range, category, text), pagination, and `client_uuid` idempotency. Amount rules: positive, two decimals, sent as strings.
- [ ] Budgets API: set / remove a GLOBAL or CATEGORY budget from a given month (upserts from Part 5-A), for both EXPENSE and INCOME
- [ ] Budget overview: allocated vs unallocated, with a warning when categories exceed the global budget
- [ ] Frontend: category management, transaction form and list, budget setup screen
- [ ] Tests: carry-over (a March budget applies in June until changed), removal via NULL, income budgets, idempotent create

**Done when:** you can record income and expenses, set monthly budgets and category allocations, change them, and see that old months keep their old limits.

## Phase 4: Recurring items (M)

- [ ] Recurring CRUD (income or expense, any category, interval unit + count, optional end date)
- [ ] Due-date function computed from `start_date` (not from the previous due date). **Unit-test month-end cases** (Jan 31, Feb 29, yearly on Feb 29).
- [ ] "Mark as paid" in one DB transaction (schema Part 5-B), with optional actual amount and payment date
- [ ] Skip, pause, end; overdue and upcoming lists
- [ ] Frontend: recurring list with due/overdue state and a mark-as-paid action
- [ ] Tests: paying twice is rejected, planned vs actual amount stored, paid transaction counts in its category's actual

**Done when:** a monthly subscription shows up as due, one click turns it into a normal transaction, and the next due date is correct.

## Phase 5: Statistics and charts (M)

- [ ] `GET /api/v1/stats/budget-vs-actual?month=` (categories + totals)
- [ ] `GET /api/v1/stats/categories/:id/trend?from=&to=`
- [ ] Dashboard: budget vs actual per category (bars/progress), month totals for income and expense, savings rate, year trend
- [ ] Extra stats from the schema: top categories, committed recurring cost per month, planned vs paid for recurring items
- [ ] Frontend formats money using the user's currency and locale
- [ ] Tests: numbers match hand-calculated examples, months with no budget return `budgeted: null`

**Done when:** the Food example works end to end: a budget of 400, transactions summing to 356.20, the chart shows both.

## Phase 6: History screens and GDPR completion (M)

- [ ] History UI: per-record history and an activity feed (from `audit_log`)
- [ ] **Export**: `export_user_data()` behind an authenticated endpoint, JSON download, plus CSV for transactions. Record in `privacy_requests`.
- [ ] **Erasure**: the daily job runs `run_scheduled_erasures()`; also allow an immediate "delete now" with re-authentication
- [ ] Cleanup jobs: expired sessions, expired verifications, optional history cap
- [ ] **Completeness test**: after `erase_user()`, no row anywhere contains that user's id, and `audit_log` and `verifications` are clean. Add a test that fails if a table with a `user_id` column is missing from the export or the erasure.
- [ ] Least-privilege database roles: the app role cannot alter or drop schema objects
- [ ] Written backup/restore note, including "after a restore, re-apply erasures recorded in `privacy_requests`"
- [ ] 1-page breach procedure (who decides, what to check, 72-hour notification rule, notifying users)
- [ ] Finalize Privacy Policy: controller contact, purposes and legal bases, retention table, processors (hosting, email, backups, Google sign-in), user rights and how to use them
- [ ] Review the data inventory against what the system really stores
- [ ] Legal review of the policy and terms if more than a few friends will use it (this plan is technical, not legal advice)

**Done when:** a user can see their history, download all their data, and delete their account, and a test proves nothing is left behind.

## Phase 7: Deploy and harden (M). Gate before inviting others

- [ ] Reverse proxy with TLS, HTTP -> HTTPS redirect, security headers, CORS locked to your frontend
- [ ] Production env: secrets from the environment, separate Google OAuth client and redirect URIs
- [ ] Database: automated encrypted backups with a tested restore; disk encryption if self-hosting
- [ ] Process manager / container restart policy; health endpoint; basic monitoring and log rotation (short retention, no personal data)
- [ ] Rate limiting and request size limits on the API
- [ ] Dependency audit in CI
- [ ] Smoke test of the whole user journey on production

**Done when:** you can restore from backup on a clean machine and a stranger could safely sign up.

## Phase 8: Mobile web (S to M)

- [ ] Responsive layouts for every screen, touch-friendly forms
- [ ] PWA: manifest and installable app, service worker for the app shell only (no offline writes yet)
- [ ] Confirm `/api/v1` is versioned and responses only ever add fields
- [ ] Test on real phones and slow connections

**Done when:** the app installs to a phone home screen and every flow works one-handed.

## Phase 9: Admin board (M)

- [ ] Add Better Auth's admin plugin (roles, ban); run its CLI migration; **keep impersonation off**
- [ ] Create the first admin by a one-off step, not through sign-up
- [ ] `admin_user_overview` view and an `admin_app` database role that can read only that view (schema Part 6.1)
- [ ] Admin API and screen: list/search users, see counts and dates, ban/unban, process a deletion request, resend verification
- [ ] `admin_audit_log` for every admin action
- [ ] Tests: the admin API never returns names, amounts, categories, descriptions, history or session IPs
- [ ] Update the privacy policy (admin access, what is visible)

**Done when:** you can manage users without being able to read anyone's finances, and every admin action is traceable.

## Phase 10: Expense splitting (L)

- [ ] **Design step first** (a short doc): groups, invites, shared expenses, shares, settlements, how a share links to a user's own transactions (schema Part 6.2)
- [ ] Decide the GDPR behaviors up front: member visibility, invite expiry, "Former member" placeholder on account deletion
- [ ] Migrations, API, balance calculation ("who owes whom"), simplified settle-up
- [ ] Frontend: groups, add shared expense, balances, settle
- [ ] Link each person's share into their own budget statistics
- [ ] Update `export_user_data()`, `erase_user()` and their tests; update the privacy policy
- [ ] Tests: deleting one member keeps the group consistent for the others

**Done when:** a group of friends can split a dinner, see balances, settle up, and a member can leave and delete their account without breaking the group.

## Phase 11: Bank connection (L)

- [ ] **Spike first** (S): choose an account-information aggregator. Check: supports your bank(s), usable by an individual/hobby project, cost, where data is processed, data-processing agreement, how long a consent lasts and how renewal works.
- [ ] Short DPIA-style note: what is imported, why, how long it is kept, who can see it
- [ ] Consent flow: record `BANK_DATA_ACCESS` in `user_consents`, show status and expiry, renewal and disconnect
- [ ] Application-level encryption of provider tokens (key outside the database), never logged
- [ ] Tables for connections, accounts and imported transactions (schema Part 6.3); additive columns on `transactions`
- [ ] Import job: fetch, deduplicate by provider transaction id, put into a PENDING review queue
- [ ] Review screen: accept, edit, ignore; category rules that pre-fill; match to recurring items ("mark as paid?")
- [ ] Update `export_user_data()`, `erase_user()` (disconnect first, then delete) and tests; update the privacy policy and processor list
- [ ] Tests: no duplicates on re-import, disconnect deletes tokens, expired consent stops imports

**Done when:** a connected account imports new transactions into a review queue, accepted ones show up in budgets, and disconnecting removes the credentials.

## Phase 12: Native mobile app (L)

- [ ] Pick the stack (e.g. Expo / React Native, sharing types and validation with the backend)
- [ ] Better Auth bearer-token plugin / Expo integration; Google sign-in with an ID token from the device; add the iOS/Android client ids to the allowed audiences
- [ ] Secure token storage on the device
- [ ] Reuse `/api/v1`; use `client_uuid` for every new transaction
- [ ] Push reminders for due recurring items (`devices` table)
- [ ] Decide on offline support (needs client-generated uuid keys, decide before building)
- [ ] App store privacy declarations match your privacy policy

**Done when:** the app signs in with Google, records a transaction in poor connectivity without duplicates, and shows the same charts as the web.

---

## Risks to watch

| Risk | Mitigation |
|---|---|
| Better Auth's generated schema differs from the plan | Reconcile in phase 1 with the CLI; treat the CLI output as the truth |
| Better Auth is still evolving quickly | Pin the version, read changelogs before upgrading, keep auth tests |
| Forgetting a table in export/erasure when adding features | The completeness test in phase 6 |
| Restoring a backup resurrects erased data | Restore note: re-apply erasures from `privacy_requests` |
| Bank access needs a regulated provider | Spike first, and keep phase 11 last among features |
| Money rounding bugs | Strings/decimals only, constraint tests, no float math |
| Scope creep | Finish and deploy phases 0 to 7 before starting any later feature |
