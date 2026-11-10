import { readFileSync } from 'node:fs'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The app's version is the one the desktop and phone builds carry (ui/package.json stays at 0.0.0). It is read here, once, and
// written into the page at build time, so showing it needs no request.
const { version } = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')) as { version: string }

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  define: { __APP_VERSION__: JSON.stringify(version) },
  // The desktop app loads this dev server, so the port and host must match src-tauri/tauri.conf.json.
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
  },
})
