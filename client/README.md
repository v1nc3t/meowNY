# meowNY client

React 19 and Vite app, shown inside the Tauri window. In development `npm run tauri dev` from the repo root starts this dev server on `http://127.0.0.1:5173` and opens the window. All data goes through Tauri commands (`src/api/`); there is no HTTP API.

From the repo root, after `npm install`:

```bash
npm run lint --workspace client
npm test --workspace client    # money, date, category and API-schema helpers
npm run build --workspace client   # typecheck, then production build into dist/
```

See the root `README.md` for the full setup.
