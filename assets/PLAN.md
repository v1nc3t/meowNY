# Expense Tracker: Build Plan

Companion to `expense_tracker_schema.txt` (schema v3). Work top to bottom. Each phase ends with a "Done when" check, so you know when to move on.

**Size tags:** S = a few hours, M = a few days, L = a week or more (rough, for one person working part-time).

## Order and why

1. **Foundations and privacy basics first**, because auth tables, ids and erasure are painful to retrofit.
2. **Core budgeting loop next**: categories, transactions, budgets. This is the product.
3. **Recurring, then charts**: both build on the core.
4. **CLI, phone layout and admin board, then GDPR completion and deployment before inviting anyone else.** Phase 10 is a gate.
5. **Features that need other people or outside parties last**: expense splitting, bank connection (riskiest last).

Phases 11 and 12 can swap places if the bank feature matters more to you than splitting. Phase 8 (phone layout) can move earlier if you mostly use the app on your phone.

## Decisions already made

- Backend: Node.js + Express, PostgreSQL 14+.
- Auth: Better Auth (email + password, Google sign-in).
- Users have uuid ids; other tables use bigint ids.
- Budgets are effective-dated; recurring items use categories; history via database triggers.
- Money is handled as strings/decimals in code, never floats.
- Query layer: Kysely. Schema stays in TypeScript migrations.
- Migrations: node-pg-migrate, TypeScript files.
- Later features (not modeled yet): expense splitting, admin board, bank connection.
- Mobile-first rule: every new screen from Phase 4 on is built mobile-first (see phase 8). Phase 3 screens are fixed up in phase 8.
- Time: months, due dates and "this month" are computed in the user's timezone (`user_settings`), never the server's.
- API contract: an OpenAPI spec is generated from the Zod schemas. The CLI and its `schema` command use it.

## Open decisions (decide when you reach the phase)

| Decision | Needed by | Notes |
|---|---|---|
| Email sender (SMTP or a provider) | Phase 2 | Needed for verification and password reset; it is a data processor |
| Hosting and backups | Phase 10 | If self-hosting: disk encryption, encrypted backups, TLS |
| Chart library | Phase 5 | Any that works with your frontend |
| History retention cap | Phase 6 | Keep forever while the account exists, or cap (e.g. 3 years)? |
| Erasure grace period | Phase 6 | Suggested 7 days; must stay well under one month |
| CLI auth token mechanism (API key vs bearer plugin) | Phase 7 | Must be revocable and stored hashed |
| Admin 2FA method (TOTP via Better Auth plugin) | Phase 9 | Required for admins; optional for users from phase 2 |
| Bank aggregator | Phase 12 | Do a spike first (see phase) |

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
- [ ] CI also runs a dependency audit and a secret scan (retrofit; do not wait for phase 10)
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
- [x] Email + password sign-up with email verification and password reset (needs the email sender)
- [ ] Google sign-in: create the OAuth client in Google Cloud Console, set redirect URIs for dev and prod, keep automatic account linking strict (verified emails only)
- [x] Privacy settings in Better Auth config: turn off IP tracking if possible, do not keep provider tokens if you can skip them, short session lifetime, secure cookies, trusted origins/CORS for your frontend only
- [x] Rate limiting on auth endpoints
- [ ] CSRF protection for cookie sessions (SameSite cookies plus an origin check or token on state-changing requests), and throttling or temporary lockout on repeated failed logins
- [ ] Basic security headers (`helmet`) and a request body size limit on the whole API
- [ ] Optional TOTP 2FA for users (Better Auth two-factor plugin)
- [x] Onboarding screen: accept Terms + Privacy Policy (`user_consents`) and choose currency (`user_settings`) in one request. Users without a `user_settings` row are sent here.
- [x] Auth middleware: load the session, put `userId` on the request, reject everything else
- [x] Account page: edit name, change currency/locale/timezone, change password, linked logins, list and revoke sessions, **request account deletion** (sets `deletion_requested_at`, signs the user out)
- [x] First draft of Terms and Privacy Policy text (versioned, e.g. `2026-10-01`)
- [x] Tests: sign-up, verification, login, Google (mocked), reset, session revoke, unauthenticated access is rejected

**Done when:** a new user can sign up by email or Google (Google sign-in is still open), finish onboarding, log in and out, reset a password, and request deletion.

## Phase 3: Core budgeting: categories, transactions, budgets (L)

- [x] Every query is scoped by `user_id` from the session, never from the request body
- [ ] Check that month filters and "current month" use the user's timezone, with a test around midnight at month end (retrofit)
- [x] Categories CRUD (category groups were built, then removed in migration 005). Soft delete writes the NULL budget row (schema Part 5-A).
- [x] Transactions CRUD with filters (date range, category, text), pagination, and `client_uuid` idempotency. Amount rules: positive, two decimals, sent as strings.
- [x] Budgets API: set / remove a GLOBAL or CATEGORY budget from a given month (upserts from Part 5-A), for both EXPENSE and INCOME
- [x] Budget overview: allocated vs unallocated, with a warning when categories exceed the global budget
- [x] Frontend: category management, transaction form and list, budget setup screen
- [x] Tests: carry-over (a March budget applies in June until changed), removal via NULL, income budgets, idempotent create

**Done when:** you can record income and expenses, set monthly budgets and category allocations, change them, and see that old months keep their old limits.

## Phase 4: Recurring items (M)

- [ ] Recurring CRUD (income or expense, any category, interval unit + count, optional end date)
- [ ] Due-date function computed from `start_date` (not from the previous due date). **Unit-test month-end cases** (Jan 31, Feb 29, yearly on Feb 29).
- [ ] "Mark as paid" in one DB transaction (schema Part 5-B), with optional actual amount and payment date
- [ ] Skip, pause, end; overdue and upcoming lists
- [ ] Due dates and "overdue" are evaluated in the user's timezone
- [ ] Frontend: recurring list with due/overdue state and a mark-as-paid action, built mobile-first
- [ ] Tests: paying twice is rejected, planned vs actual amount stored, paid transaction counts in its category's actual

**Done when:** a monthly subscription shows up as due, one click turns it into a normal transaction, and the next due date is correct.

## Phase 5: Statistics and charts (M)

- [ ] `GET /api/v1/stats/budget-vs-actual?month=` (categories + totals)
- [ ] `GET /api/v1/stats/categories/:id/trend?from=&to=`
- [ ] Dashboard (built mobile-first): budget vs actual per category (bars/progress), month totals for income and expense, savings rate, year trend
- [ ] Extra stats from the schema: top categories, committed recurring cost per month, planned vs paid for recurring items
- [ ] Frontend formats money using the user's currency and locale
- [ ] Tests: numbers match hand-calculated examples, months with no budget return `budgeted: null`

**Done when:** the Food example works end to end: a budget of 400, transactions summing to 356.20, the chart shows both.

## Phase 6: History screens and GDPR completion (M)

- [ ] History UI: per-record history and an activity feed (from `audit_log`)
- [ ] **Export**: `export_user_data()` behind an authenticated endpoint that needs re-authentication and is rate-limited. Direct JSON download only (never an emailed link), plus CSV for transactions. Record in `privacy_requests`.
- [ ] CSV safety: cells starting with `=`, `+`, `-` or `@` are prefixed with `'` on export, and rejected or neutralised on import (phase 7)
- [ ] **Erasure**: the daily job runs `run_scheduled_erasures()`; also allow an immediate "delete now" with re-authentication
- [ ] Cleanup jobs: expired sessions, expired verifications, optional history cap
- [ ] **Completeness test**: after `erase_user()`, no row anywhere contains that user's id, and `audit_log` and `verifications` are clean. Add a test that fails if a table with a `user_id` column is missing from the export or the erasure.
- [ ] Least-privilege database roles: the app role cannot alter or drop schema objects
- [ ] Security event log: logins, failed logins, token use, exports, deletions and (later) admin actions, with no personal data and short retention. This feeds the breach procedure in phase 10.
- [ ] Finalize Privacy Policy: controller contact, purposes and legal bases, retention table, processors (hosting, email, backups, Google sign-in), user rights and how to use them
- [ ] Review the data inventory against what the system really stores

**Done when:** a user can see their history, download all their data, and delete their account, and a test proves nothing is left behind.

## Phase 7: CLI (M)

The whole app is usable from the terminal, so scripts and AI agents can drive it without the web UI.

- [ ] Design rule: the CLI is a thin client over `/api/v1`, built on the OpenAPI spec generated from the Zod schemas. No direct database access, so user scoping, validation and GDPR rules stay in one place. Every feature through phase 6 gets a command; if the API lacks something, fix the API, not the CLI.
- [ ] Package in `cli/` (TypeScript, one binary, e.g. `meowny`). Reuse the server's Zod schemas for argument validation.
- [ ] Auth: API token created on the account page (revocable, stored hashed, listed with sessions), passed by `MEOWNY_TOKEN` env var or `meowny login`. Never print or log the token. Decide the mechanism (Better Auth API key plugin or bearer plugin) at the start of the phase.
- [ ] Token limits: scopes with read-only as the default and write as opt-in; an expiry; last-used time shown; per-token rate limits. `delete-account`, `export` and password change are not allowed with a token and need a fresh interactive re-auth. The CLI stores its token file with 0600 permissions.
- [ ] Command groups: `account`, `categories`, `transactions`, `budgets`, `recurring`, `stats`, `history`, `export`, `delete-account`. Pattern `meowny <noun> <verb>`, with `--help` at every level.
- [ ] Agent-friendly output: `--json` on every command (stable shape, additive fields only), human table otherwise. Data on stdout, messages and errors on stderr.
- [ ] Non-interactive by default: no prompts; every input is a flag or stdin. Destructive commands need `--yes`. Transaction create sends a `client_uuid`, so retries are safe.
- [ ] Exit codes: 0 ok, 1 error, 2 bad usage, 3 auth failed, 4 not found. In `--json` mode errors are `{code, message}`.
- [ ] Money stays strings end to end; dates are ISO 8601. Bulk transaction import from CSV/JSON on stdin, with the CSV safety rules from phase 6.
- [ ] `meowny schema` prints all commands and arguments as JSON (from the OpenAPI spec), so an agent can discover them.
- [ ] Docs: one `CLI.md` with an example per command, plus a short agent note (env var, `--json`, exit codes).
- [ ] Tests: each command against a real test server (happy path, validation error, unauthenticated, `--json` shape snapshot), plus one test that fails if an API route in scope is missing from the spec or has no command.
- [ ] From here on, every new feature phase ships its CLI commands in the same phase.
- [ ] Data inventory: add API tokens to the inventory and to the export/erasure completeness test. Token use goes into the security event log.

**Done when:** a script or agent holding only a token can record an expense, set a budget, read budget vs actual and export all data, all with `--json` and no prompts.

## Phase 8: Phone layout (S to M)

The web app is rearranged for phone aspect ratios (tall and narrow, about 360 to 430 px wide), not just shrunk. Screens built from phase 4 on are already mobile-first, so this phase fixes up the phase 3 screens and adds the phone-specific parts.

- [ ] Mobile-first layouts for every screen: single column, sticky bottom tab bar instead of a sidebar, tables become cards or lists, charts resize to the width
- [ ] Phone-first quick add: a floating "add transaction" button, amount field first with a numeric keypad (`inputmode="decimal"`), recent categories on top
- [ ] Touch-friendly: tap targets at least 44 px, bottom sheets instead of modals, swipe or long-press for edit and delete, no hover-only actions
- [ ] Handle phone quirks: safe-area insets (notch, home bar), `100dvh` for the on-screen keyboard, no zoom on input focus (inputs at least 16 px), portrait and landscape both usable
- [ ] PWA: manifest and installable app, service worker for the app shell only (no offline writes yet). It never caches API responses, so financial data does not sit on the device, and logout clears its caches.
- [ ] Confirm `/api/v1` is versioned and responses only ever add fields
- [ ] Test on real phones (small and large) and slow connections; check at 360, 390 and 430 px widths

**Done when:** the app installs to a phone home screen, no screen scrolls sideways, and every flow works one-handed.

## Phase 9: Admin board (M)

- [ ] Add Better Auth's admin plugin (roles, ban); run its CLI migration; **keep impersonation off**
- [ ] Create the first admin by a one-off step, not through sign-up
- [ ] 2FA required for every admin account; admin sessions are short
- [ ] `admin_user_overview` view and an `admin_app` database role that can read only that view (schema Part 6.1)
- [ ] Admin API and screen: list/search users, see counts and dates, ban/unban, process a deletion request, resend verification
- [ ] `admin_audit_log` for every admin action, also written to the security event log
- [ ] Tests: the admin API never returns names, amounts, categories, descriptions, history or session IPs
- [ ] Update the privacy policy (admin access, what is visible)

**Done when:** you can manage users without being able to read anyone's finances, and every admin action is traceable.

## Phase 10: Deploy and harden (M). Gate before inviting others

- [ ] Reverse proxy with TLS, HTTP -> HTTPS redirect, security headers checked on the live site, CORS locked to your frontend
- [ ] Production env: secrets from the environment, separate Google OAuth client and redirect URIs
- [ ] Database: automated encrypted backups with a tested restore; disk encryption if self-hosting
- [ ] Process manager / container restart policy; health endpoint; basic monitoring and log rotation (short retention, no personal data)
- [ ] Rate limiting on the whole API (per IP and per user), on top of the auth limits
- [ ] Staging environment that mirrors production (own database, own secrets, own OAuth client). Restore tests and the smoke test run there first.
- [ ] Email deliverability: SPF, DKIM and DMARC set up for the sender; send a real verification and reset email to a major provider and check it does not land in spam
- [ ] Written backup/restore note, including "after a restore, re-apply erasures recorded in `privacy_requests`"
- [ ] 1-page breach procedure (who decides, what to check using the security event log, 72-hour notification rule, notifying users)
- [ ] Legal review of the policy and terms if more than a few friends will use it (this plan is technical, not legal advice)
- [ ] Smoke test of the whole user journey on staging, then production

**Done when:** you can restore from backup on a clean machine and a stranger could safely sign up.

## Phase 11: Expense splitting (L)

- [ ] **Design step first** (a short doc): groups, invites, shared expenses, shares, settlements, how a share links to a user's own transactions (schema Part 6.2)
- [ ] Decide the GDPR behaviors up front: member visibility, invite expiry, "Former member" placeholder on account deletion
- [ ] Authorization is by group membership, not just `user_id`. Invite tokens are single-use, expiring and stored hashed.
- [ ] Migrations, API, balance calculation ("who owes whom"), simplified settle-up
- [ ] Frontend: groups, add shared expense, balances, settle
- [ ] Link each person's share into their own budget statistics
- [ ] Update `export_user_data()`, `erase_user()` and their tests; update the privacy policy
- [ ] Tests: deleting one member keeps the group consistent for the others; a non-member gets 404 for every group, expense and balance route; an expired or reused invite is rejected

**Done when:** a group of friends can split a dinner, see balances, settle up, and a member can leave and delete their account without breaking the group.

## Phase 12: Bank connection (L)

Read-only account information through an Open Banking aggregator (e.g. GoCardless, TrueLayer, Plaid). The user logs in at the bank or in the aggregator's hosted widget (OAuth 2.0 redirect). Our frontend and backend never see bank usernames, passwords, MFA codes or card numbers. We hold only a scoped, read-only, revocable token.

- [ ] **Spike first** (S): choose the aggregator. Check: supports your bank(s), usable by an individual/hobby project, cost, where data is processed, SOC 2 or equivalent audit and Open Banking authorisation, data-processing agreement, consent length and renewal, what production access requires (privacy policy, security basics, consent flow).
- [ ] Short DPIA-style note: what is imported, why, how long it is kept, who can see it, aggregator as a processor
- [ ] Connect flow: backend creates a link session, frontend redirects to or opens the aggregator widget, callback exchanges the result for a token. Validate a one-time `state` value per user on the callback. Never accept credentials in our own forms.
- [ ] Consent: record `BANK_DATA_ACCESS` in `user_consents`, show status and expiry, renewal and disconnect
- [ ] **Token protection (the main risk is database theft):** encrypt provider tokens at rest with AES-256-GCM at application level, key from the environment or a secrets manager (never in the database or git), never logged or returned by the API, and not included in `export_user_data()`
- [ ] Least scope: read-only accounts and transactions only, no payment initiation
- [ ] Key rotation: store a key id with each encrypted token so the key can be changed and tokens re-encrypted
- [ ] If the aggregator sends webhooks: verify the signature and reject replays
- [ ] Currency: decide how accounts in a currency other than the user's are handled (store the original currency and amount; do not convert silently)
- [ ] Imported merchant names and descriptions are untrusted text: escape on render and neutralise in CSV export
- [ ] Tables for connections, accounts and imported transactions (schema Part 6.3); additive columns on `transactions`. Every query scoped by `user_id`, same as the rest of the API.
- [ ] Import job: fetch, deduplicate by provider transaction id, put into a PENDING review queue. Handle token expiry and aggregator errors by marking the connection `NEEDS_REAUTH` and notifying the user.
- [ ] Disconnect and revoke: user can disconnect at any time. This revokes the token at the aggregator first, then deletes it locally. Erasure and consent expiry do the same.
- [ ] Review screen: accept, edit, ignore; category rules that pre-fill; match to recurring items ("mark as paid?")
- [ ] Backups: confirm encrypted tokens and imported data are covered by the backup note, and the encryption key is stored separately from backups
- [ ] CLI: commands for connections, review queue (list, accept, ignore) and disconnect
- [ ] Update `export_user_data()`, `erase_user()` (revoke and disconnect first, then delete) and tests; update the privacy policy and processor list (aggregator)
- [ ] Tests: no duplicates on re-import, disconnect revokes and deletes the token, expired consent stops imports, token is never present in logs, API responses or export, a key rotation keeps connections working, a user cannot see another user's imported transactions

**Done when:** a connected account imports new transactions into a review queue, accepted ones show up in budgets, and disconnecting revokes the token at the aggregator and removes it from our database.

---

## Risks to watch

| Risk | Mitigation |
|---|---|
| Better Auth's generated schema differs from the plan | Reconcile in phase 1 with the CLI; treat the CLI output as the truth |
| Better Auth is still evolving quickly | Pin the version, read changelogs before upgrading, keep auth tests |
| Forgetting a table in export/erasure when adding features | The completeness test in phase 6 |
| Restoring a backup resurrects erased data | Restore note: re-apply erasures from `privacy_requests` |
| Bank access needs a regulated provider | Spike first, and keep phase 12 last among features |
| Stolen database exposes bank tokens | Encrypt tokens at rest, key outside the database, read-only scope, revoke on disconnect |
| Cookie sessions without CSRF protection | CSRF rules in phase 2, tests for state-changing requests |
| CLI token leaked or over-privileged | Read-only default scope, expiry, hashed storage, re-auth for destructive actions |
| Money rounding bugs | Strings/decimals only, constraint tests, no float math |
| Scope creep | Finish and deploy phases 0 to 10 before starting any later feature |
