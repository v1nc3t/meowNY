# Data inventory

What personal data this app is built to hold, why, where it lives, how long, and who else processes it. Update this page in every phase. This is a technical note, not a privacy policy.

Status at the end of phase 0: the API stores nothing. No account can be created yet. The rows below are the schema planned for phase 1. Nothing in this list is collected until that schema is in use.

| Data | Purpose | Where | Retention | Processor |
|---|---|---|---|---|
| Name, email, email verified, optional picture URL | Account and login | `users` | While the account exists | Not collected |
| Deletion request time | Erase the account after a short grace period | `users.deletion_requested_at` | Until erasure runs | Not collected |
| Session token, expiry, IP address, user agent | Stay signed in, show sessions | `sessions` | Until session expiry, then a short purge | Not collected |
| Login provider, provider account id, password hash, OAuth tokens | Email or Google sign-in | `accounts` | While the account exists. Tokens should not be kept if sign-in does not need them | Not collected |
| Verification identifier and token | Email verification and password reset | `verifications` | Until expiry | Not collected |
| Currency, locale, timezone | Display money and dates. Amounts are not converted when currency changes | `user_settings` | While the account exists | Not collected |
| Consent purpose, policy version, granted and withdrawn times | Record which terms were accepted | `user_consents` | While the account exists | Not collected |
| Privacy request type, status, times, orphaned user id | Proof a request was handled. No name or email | `privacy_requests` | Kept after erasure (id only) | Not collected |
| Category groups, categories, budgets, transactions, recurring items | The budgeting product. Names and descriptions can reveal sensitive facts | those tables | While the account exists | Not collected |
| History of creates, edits, and deletes | The user's own change history | `audit_log` | While the account exists, then erased with the account | Not collected |

Server logs: method, path, status, duration. No body, query string, headers, or row contents.

Processors today: none. Local Postgres in development is the only database, and it holds no accounts yet.
