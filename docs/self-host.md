# Self-hosting meowNY

`meowny-server` is meowNY served to a browser: the same app and the same encrypted data file, on a machine you run, for **one
person**. You open it in any browser instead of installing the desktop app.

## Run it

```sh
git clone https://github.com/v1nc3t/meowNY.git
cd meowNY
docker compose up -d
```

1. Open <http://127.0.0.1:8080>.
2. Press **get started**, or **restore from a backup** to start from a desktop backup.
3. Choose a passphrase. It is required: it opens the data and signs you in.
4. Save the recovery key. **download** saves it as a text file.

To update, run `git pull`, then `docker compose up -d --build`. The data stays in the `meowny-data` volume.

Without compose:

```sh
docker build -t meowny .
docker run -d --name meowny --restart unless-stopped \
  -p 127.0.0.1:8080:8080 -v meowny-data:/data -v meowny-backups:/backups \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true meowny
```

## Reach it from other devices

The compose file listens on the host only (`127.0.0.1`). To use it from your phone or another computer, put a reverse proxy with
HTTPS in front of it. Do not publish port 8080 to a network: the passphrase would cross it in plain text.

With [Caddy](https://caddyserver.com), which gets the certificate itself:

```caddy
budget.example.org {
    reverse_proxy 127.0.0.1:8080
}
```

With nginx, pass the scheme on so the session cookie is marked `Secure`, and allow uploads as large as your backups:

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    client_max_body_size 1g;
}
```

On a home network only, a VPN such as WireGuard or Tailscale is simpler than a public name.

## Settings

All are environment variables. The image sets the first four.

| Variable | Default (image) | What it does |
| --- | --- | --- |
| `MEOWNY_LISTEN` | `0.0.0.0:8080` | Address and port to listen on. Outside the image the default is `127.0.0.1:8080`. |
| `MEOWNY_DATA_DIR` | `/data` | The encrypted data, its secret and the documents. |
| `MEOWNY_BACKUP_DIR` | `/backups` | Where scheduled backups are written. |
| `MEOWNY_WEB_DIR` | `/app/web` | The built page (`ui/dist`, built with `MEOWNY_SERVED=1`). |
| `MEOWNY_COOKIE_SECURE` | `auto` | `auto` marks the cookie `Secure` when the proxy sends `X-Forwarded-Proto: https`. `true` always does; `false` never does. |
| `MEOWNY_MAX_UPLOAD_MB` | `1024` | The largest file you can upload (a backup to restore). |
| `MEOWNY_THREADS` | `8` | Requests handled at the same time. |

## What differs from the desktop app

- **The passphrase is the login.** Unlocking gives the browser a session cookie (HttpOnly, SameSite=Strict). Each browser signs in
  with the passphrase, even when the data is already open in another one.
- **A lock signs everyone out.** **lock**, the idle lock (**settings** › **app lock**) and deleting all data end every session.
- **Files go through the browser.** Exports, backups, the recovery key and document copies download. To restore a backup, you pick
  the file in the browser. **open** on a document downloads a copy.
- **Scheduled backups** go to `MEOWNY_BACKUP_DIR`. Mount a volume on another disk there.
- **Left out:** pairing and syncing a phone, the command line API, notifications, and dropping files on the window. The server
  refuses their commands.
- **No erase after wrong passphrases.** The phone erases its data after five wrong tries. Anyone who can reach the page could use
  that to wipe your data, so a server never erases. Each wrong passphrase waits a second instead. Use a long passphrase, and add
  rate limits at the proxy if the page is public.

## Security notes

- The data file is encrypted. Its key is wrapped with the passphrase, and the secret is in `server.secret` in the data folder.
  Without the passphrase or the recovery key, a copy of the volume is unreadable. With them, it can be read.
- Whoever opens a new server first sets it up. Set it up before you make it reachable.
- Exports are not encrypted. Before download they wait in `/tmp` (a tmpfs in the compose file). Each one can be fetched once and
  is deleted after 15 minutes.
- The server logs no requests and no data. The page's Content-Security-Policy lets it talk to its own server only.

## How it works

`selfhost/` is a small HTTP server (tiny_http) around the app's own library. It runs the app's commands
through Tauri's mock runtime, the one the tests use, by the same names and arguments the desktop window sends. The business logic
is shared, not copied. `ui/src/api/served.ts` is the page's side of it: `POST /api/invoke/<command>` with a JSON body and an
`X-Meowny: 1` header. A file dialog becomes `POST /api/upload` before the command or `GET /api/download/<id>` after it
(`src-tauri/src/served.rs`).

To work on it without Docker:

```sh
cargo build --manifest-path selfhost/Cargo.toml
MEOWNY_SERVED=1 npm run build --workspace ui
MEOWNY_DATA_DIR=./.tmp/data MEOWNY_WEB_DIR=ui/dist ./selfhost/target/debug/meowny-server
# for the page with live reload, in another terminal: MEOWNY_SERVED=1 npm run dev --workspace ui (proxies /api to :8080)
```
