import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The app's version is the one the desktop and phone builds carry (ui/package.json stays at 0.0.0). It is read here, once, and
// written into the page at build time, so showing it needs no request.
const { version } = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')) as { version: string }

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  // MEOWNY_SERVED=1 builds the page for `meowny-server` (docs/self-host.md), which talks to it over HTTP instead of through the window.
  define: { __APP_VERSION__: JSON.stringify(version), __MEOWNY_SERVED__: JSON.stringify(process.env.MEOWNY_SERVED === '1') },
  // The desktop app loads this dev server, so the port and host must match src-tauri/tauri.conf.json.
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    // `MEOWNY_SERVED=1 npm run dev` works on the page against a `meowny-server` running on this port.
    proxy: { '/api': 'http://127.0.0.1:8080' },
  },
})
