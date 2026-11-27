# meowNY, self-hosted: the app served to a browser by `meowny-server` (docs/self-host.md).
#
#   docker build -t meowny .
#   docker run -d --name meowny -p 127.0.0.1:8080:8080 -v meowny-data:/data meowny
#
# Or `docker compose up -d` with the compose.yaml beside this file.

# --- the page --------------------------------------------------------------------------------------------------------------------
FROM node:22-bookworm-slim AS page
WORKDIR /src
COPY package.json package-lock.json ./
COPY ui/package.json ui/
# the root holds TypeScript, which the page's build uses
RUN npm ci --no-audit --no-fund
COPY ui ui
# vite reads the app's version from here
COPY src-tauri/tauri.conf.json src-tauri/
RUN MEOWNY_SERVED=1 npm run build --workspace ui

# --- the server ------------------------------------------------------------------------------------------------------------------
FROM rust:1-bookworm AS server
# Tauri's system libraries, as in CI: the server runs the app's commands through Tauri, though it never opens a window.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY src-tauri src-tauri
COPY selfhost selfhost
# the app's build embeds the page it was built with
COPY --from=page /src/ui/dist ui/dist
RUN cargo build --release --locked --manifest-path selfhost/Cargo.toml \
 && cp selfhost/target/release/meowny-server /usr/local/bin/meowny-server

# --- what runs -------------------------------------------------------------------------------------------------------------------
FROM debian:bookworm-slim
# GTK is linked by Tauri even without a window; tini passes the stop signal on and reaps children.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libgtk-3-0 libdbus-1-3 tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin meowny \
 && mkdir -p /data /backups \
 && chown meowny:meowny /data /backups \
 && chmod 700 /data /backups
COPY --from=server /usr/local/bin/meowny-server /usr/local/bin/meowny-server
COPY --from=page /src/ui/dist /app/web
ENV MEOWNY_LISTEN=0.0.0.0:8080 \
    MEOWNY_DATA_DIR=/data \
    MEOWNY_BACKUP_DIR=/backups \
    MEOWNY_WEB_DIR=/app/web
USER meowny
VOLUME ["/data", "/backups"]
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/meowny-server"]
