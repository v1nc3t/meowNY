# meowNY server

Express API for meowNY. Phase 0 serves a health check and runs SQL migrations against Postgres.

## Run

From the repo root:

```bash
docker compose up -d
cp .env.example .env   # skip if .env already has DATABASE_URL
cd server
npm install
npm run migrate
npm run dev
```

`GET http://127.0.0.1:3000/health` returns `{ "status": "ok" }` when Postgres answers. `GET /api/v1/me` without a session returns 401. Better Auth is mounted at `/api/auth`.

`docker compose up -d` also starts Mailpit. Verification and password-reset messages show up at `http://127.0.0.1:8025`. SMTP defaults to `127.0.0.1:1025`.

`docker compose` creates `meowny` and, on a new volume, `meowny_test`. Tests also create `meowny_test` if it is missing. An old Docker volume ignores the init script. `docker compose down -v` once, then `up -d`, if the test database is missing and you do not want the test helper to create it.

`npm test` uses `DATABASE_URL_TEST`.

`npm run seed` loads a demo user, categories, budgets, and transactions into `DATABASE_URL`. That user has no password. `npm run db:types` regenerates `src/db/schema.ts` after a schema change.
