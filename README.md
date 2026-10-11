# meowNY

Expense tracker. A React client talks to an Express API, and the API stores data in Postgres.

You need Node.js 22 or newer, npm, and Docker.

## First-time setup

From the repo root:

```bash
cp .env.example .env
docker compose up -d
npm install
cd server && npm run migrate
```

`docker compose up -d` starts Postgres on `127.0.0.1:5432` and Mailpit. Mailpit catches signup and password-reset mail. Open the inbox at http://127.0.0.1:8025.

Copy `.env.example` only once. Generate `BETTER_AUTH_SECRET` with `openssl rand -base64 32` and put it in `.env` before you sign anyone up.

On a brand-new Docker volume, Compose also creates the `meowny_test` database. An old volume skips that init script. Run `docker compose down -v` and then `docker compose up -d` if tests cannot find `meowny_test` and you are fine losing the local database.

## Development

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

## Tests and checks

Server tests need Compose running. They use `DATABASE_URL_TEST` and do not write to the app database.

```bash
cd server && npm run lint && npm run typecheck && npm test
cd ../client && npm run lint && npm run build && npm test
```

CI runs the same commands on every push.

## Production-style start

Build both packages, then run the API from its compiled output. Serve the client build with any static host that forwards `/api` to the API.

```bash
cd server && npm run build && npm start
cd ../client && npm run build
```

`npm start` reads `../.env`. Set `BETTER_AUTH_URL` and `FRONTEND_ORIGIN` to the public API and site origins before you use this outside your machine.

Migrations run through `npm run migrate` (it needs the dev dependencies, so run it from a full `npm install` checkout). `npm start` does not migrate on its own.
