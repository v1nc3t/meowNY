# Data inventory

What personal data this app is built to hold, why, where it lives, how long, and who else processes it. Update this page in every phase. This is a technical note, not a privacy policy.

Status: email sign-up sends a verification link, and password reset sends a reset link. In development those messages go to local Mailpit over SMTP. The message contains the address and the link. Google sign-in is not connected. The demo seed is fixture data, not a person.

| Data | Purpose | Where | Retention | Processor |
|---|---|---|---|---|
| Name, email, email verified, optional picture URL | Account and login | `users` | While the account exists | Local Postgres (the operator's machine in development) |
| Deletion request time | Erase the account after a short grace period | `users.deletion_requested_at` | Until erasure runs | Local Postgres (the operator's machine in development) |
| Session token, expiry, user agent | Stay signed in, show sessions. The client IP is used for rate limiting in process memory and is not written to the session row | `sessions` | Until session expiry, then a short purge | Local Postgres (the operator's machine in development) |
| Login provider, provider account id, password hash | Email or Google sign-in. Provider access, refresh, and id tokens are cleared before save | `accounts` | While the account exists | Local Postgres (the operator's machine in development) |
| Verification identifier and token | Email verification and password reset. The message (address and link) is handed to the configured SMTP server | `verifications` | Until expiry | Local Postgres, plus local Mailpit in development |
| Currency, locale, timezone | Display money and dates. Amounts are not converted when currency changes | `user_settings` | While the account exists | Local Postgres (the operator's machine in development) |
| Consent purpose, policy version, granted and withdrawn times | Record which terms were accepted | `user_consents` | While the account exists | Local Postgres (the operator's machine in development) |
| Privacy request type, status, times, orphaned user id | Proof a request was handled. No name or email | `privacy_requests` | Kept after erasure (id only) | Local Postgres (the operator's machine in development) |
| Category groups, categories, budgets, transactions, recurring items | The budgeting product. Names and descriptions can reveal sensitive facts | those tables | While the account exists | Local Postgres (the operator's machine in development) |
| History of creates, edits, and deletes | The user's own change history | `audit_log` | While the account exists, then erased with the account | Local Postgres (the operator's machine in development) |

Server logs: method, path, status, duration, and that a mail was sent. No body, query string, headers, address, or link.

Processor: the Postgres database this app is pointed at, and the SMTP server in `SMTP_HOST`. In development both are local Docker (Postgres and Mailpit). Server logs stay method, path, status, and duration, plus a line that mail was sent. They do not include the address or the link.
