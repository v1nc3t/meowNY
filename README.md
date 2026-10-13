# meowNY

Local-first expense tracker. A Tauri desktop app (Rust) shows a React client and keeps all data in an encrypted SQLite file (SQLCipher) on your own machine. No server, no accounts, no telemetry. See `assets/PLAN.md` for the roadmap and `assets/data-inventory.md` for what data lives where.

The app is mid-migration. The database, key handling, app lock and settings are done (phases 0 to 2). The budgeting screens (categories, transactions, budgets) still use the old Express/Postgres server until phase 3 moves them over. See [Old server (temporary)](#old-server-temporary).

## Requirements

- Node.js 22 or newer and npm
- Rust (stable, 1.77 or newer) from https://rustup.rs
- Tauri's system libraries for your OS: https://v2.tauri.app/start/prerequisites/
  - Linux (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential pkg-config`
  - macOS: Xcode command line tools
  - Windows: Microsoft C++ Build Tools and WebView2
- A secret service for the key: macOS Keychain, Windows Credential Manager, or on Linux GNOME Keyring / KWallet running in your session. Without one, first run fails with "key unavailable".

## Run the app

From the repo root:

```bash
npm install
npm run tauri dev
```

The first run compiles the Rust side (a few minutes). It starts the Vite dev server for the client and opens a window; edits to `client/src` hot-reload, edits to `src-tauri/src` rebuild and restart the app.

On first launch the app asks for a currency and an optional passphrase, then creates the encrypted data file. Settings (currency, locale, timezone, app lock, delete all data) are at `/app/settings`.

Running `npm run dev --workspace client` on its own opens the client in a browser. That mode has no vault and no backend: the lock, first-run, settings and category screens only work in the desktop window. Pages not yet moved (transactions, budgets) still use the old server there.

### Where the data lives

| OS | Folder |
|---|---|
| Linux | `~/.local/share/org.meowny.app/` |
| macOS | `~/Library/Application Support/org.meowny.app/` |
| Windows | `%APPDATA%\org.meowny.app\` |

It holds `meowny.db` (encrypted) and `vault.json` (lock settings and the passphrase salt, nothing secret). The key is in the OS keychain under the service name `org.meowny.app`.

To start over in development, use Settings, delete all my data. That removes the files and the keychain entry together. Deleting the folder by hand leaves a stale keychain entry, which is harmless and gets replaced on the next first run.

## Tests and checks

```bash
# Rust: migrations, constraints, vault, key handling
cd src-tauri && cargo test

# Client
npm run lint --workspace client
npm test --workspace client
npm run build --workspace client

# Lists every place that can make a network call and fails on new ones
npm run check:outbound
```

CI (`.github/workflows/desktop.yml`) runs the outbound check and `cargo test`, then builds the app on Linux, macOS and Windows.

## Database

Schema migrations are plain SQL in `src-tauri/migrations/`, embedded in the app and applied on startup. The version is stored in SQLite's `user_version`, and a database from a newer app version is refused. Add a change as the next numbered file and append it to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Never edit a migration that has shipped.

Money is stored as integer minor units (`*_minor` columns). Constraint tests are in `src-tauri/src/schema_tests.rs`.

### Demo data

```bash
cd src-tauri
cargo run --example seed -- /path/to/dev.db <64-hex-character-key>
```

This creates a separate encrypted database with demo categories, budgets, recurring items and transactions, using the key you pass. The desktop app does not open it: the app derives its own key from the OS keychain. Use the seed file for inspecting the schema and for tests. It refuses a database that already has data.

## Old server (temporary)

The `server/` package (Express, Better Auth, Postgres) and the pages that call it (login, signup, dashboard, transactions, categories, budgets, account) are being replaced in phase 3, then deleted. Until then they still work in a browser. These notes are for that code only.

### First-time setup

From the repo root:

```bash
cp .env.example .env
docker compose up -d
npm install
cd server && npm run migrate
```

`docker compose up -d` starts Postgres on `127.0.0.1:5432` and Mailpit. Mailpit catches signup and password-reset mail, so in development no mail reaches a real inbox. Open the verification link from http://127.0.0.1:8025.

To send real mail, set `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, and `SMTP_FROM` in `.env` to your provider's values, then restart the API.

Copy `.env.example` only once. Generate `BETTER_AUTH_SECRET` with `openssl rand -base64 32` and put it in `.env` before you sign anyone up.

On a brand-new Docker volume, Compose also creates the `meowny_test` database. An old volume skips that init script. Run `docker compose down -v` and then `docker compose up -d` if tests cannot find `meowny_test` and you are fine losing the local database.

### Development

Start the API and the client in two terminals. The client proxies `/api` to the API, so the browser only talks to port 5173.

```bash
# terminal 1
cd server && npm run dev
```

```bash
# terminal 2
cd client && npm run dev
```

Open http://127.0.0.1:5173 and create an account. The API listens on http://127.0.0.1:3000. `GET /health` returns `{ "status": "ok" }` when Postgres answers.

Optional demo rows (a user, categories, budgets, and transactions) go into the database with `npm run seed` from `server`. That demo user has no password and cannot sign in.

Google sign-in stays off until both `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` are set. For local development, the OAuth redirect URI is `http://127.0.0.1:3000/api/auth/callback/google` and the JavaScript origin is `http://127.0.0.1:5173`.

### Tests and checks

Server tests need Compose running. They use `DATABASE_URL_TEST` and do not write to the app database.

```bash
cd server && npm run lint && npm run typecheck && npm test
cd ../client && npm run lint && npm run build && npm test
```

CI runs the same commands on every push.

### Production-style start

Build both packages, then run the API from its compiled output. Serve the client build with any static host that forwards `/api` to the API.

```bash
cd server && npm run build && npm start
cd ../client && npm run build
```

`npm start` reads `../.env`. Set `BETTER_AUTH_URL` and `FRONTEND_ORIGIN` to the public API and site origins before you use this outside your machine.

Migrations run through `npm run migrate` (it needs the dev dependencies, so run it from a full `npm install` checkout). `npm start` does not migrate on its own.
