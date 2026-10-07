# Data inventory

What personal data this app is built to hold, why, where it lives, how long, and who else processes it. Update this page in every phase. This is a technical note, not a privacy policy.

Status at the end of phase 1: the tables exist in Postgres. The API still has no sign-up, so the only rows are whatever a developer seeds locally. The demo seed is fixture data, not a person. No email provider, Google, or host is connected.

| Data | Purpose | Where | Retention | Processor |
|---|---|---|---|---|
| Name, email, email verified, optional picture URL | Account and login | `users` | While the account exists | Local Postgres (the operator's machine in development) |
| Deletion request time | Erase the account after a short grace period | `users.deletion_requested_at` | Until erasure runs | Local Postgres (the operator's machine in development) |
| Session token, expiry, IP address, user agent | Stay signed in, show sessions | `sessions` | Until session expiry, then a short purge | Local Postgres (the operator's machine in development) |
| Login provider, provider account id, password hash, OAuth tokens | Email or Google sign-in | `accounts` | While the account exists. Tokens should not be kept if sign-in does not need them | Local Postgres (the operator's machine in development) |
| Verification identifier and token | Email verification and password reset | `verifications` | Until expiry | Local Postgres (the operator's machine in development) |
| Currency, locale, timezone | Display money and dates. Amounts are not converted when currency changes | `user_settings` | While the account exists | Local Postgres (the operator's machine in development) |
| Consent purpose, policy version, granted and withdrawn times | Record which terms were accepted | `user_consents` | While the account exists | Local Postgres (the operator's machine in development) |
| Privacy request type, status, times, orphaned user id | Proof a request was handled. No name or email | `privacy_requests` | Kept after erasure (id only) | Local Postgres (the operator's machine in development) |
| Category groups, categories, budgets, transactions, recurring items | The budgeting product. Names and descriptions can reveal sensitive facts | those tables | While the account exists | Local Postgres (the operator's machine in development) |
| History of creates, edits, and deletes | The user's own change history | `audit_log` | While the account exists, then erased with the account | Local Postgres (the operator's machine in development) |

Server logs: method, path, status, duration. No body, query string, headers, or row contents.

Processor: the Postgres database this app is pointed at. In development that is the local Docker database. Nothing else receives this data. Server logs stay method, path, status, and duration.
