#!/usr/bin/env node
// Lists every place that can make an outbound network call, or accept an inbound one, and fails when one is
// not in scripts/outbound-allowlist.txt. Adding network code therefore needs a visible, reviewed allowlist
// entry with a reason. Stale entries fail too, so the list stays honest.
//
//   node scripts/check-outbound.mjs          check (used by CI)
//   node scripts/check-outbound.mjs --list   print what was found

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(fileURLToPath(import.meta.url), '..', '..')

const SOURCE_RULES = [
  ['fetch', /\bfetch\s*\(/],
  ['xhr', /\bXMLHttpRequest\b/],
  ['websocket', /\bWebSocket\b/],
  ['eventsource', /\bEventSource\b/],
  ['beacon', /\bsendBeacon\b/],
  ['url-literal', /https?:\/\/(?!tauri\.localhost|ipc\.localhost)[a-z0-9]/i],
  ['network-package', /from\s+['"](better-auth[^'"]*|axios|socket\.io-client|ws)['"]/],
]
const RUST_RULES = [
  ['url-literal', /https?:\/\/[a-z0-9]/i],
  ['network-crate', /\b(reqwest|ureq|hyper|isahc|surf|attohttpc|tokio_tungstenite|tungstenite)\b/],
  ['std-net', /\b(TcpStream|UdpSocket)\b/],
  ['listener', /\bTcpListener\b/],
]
const NETWORK_CRATES = /^(reqwest|ureq|hyper|curl|isahc|surf|attohttpc|tauri-plugin-http|tauri-plugin-updater|tauri-plugin-websocket)\b/

function walk(dir, accept) {
  const out = []
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'target' || name === 'dist') continue
    const path = join(dir, name)
    if (statSync(path).isDirectory()) out.push(...walk(path, accept))
    else if (accept(name)) out.push(path)
  }
  return out
}

const found = new Set()
function scan(files, rules) {
  for (const file of files) {
    const rel = relative(root, file).split('\\').join('/')
    for (const line of readFileSync(file, 'utf8').split('\n')) {
      const code = line.trim()
      if (code.startsWith('//') || code.startsWith('*') || code.startsWith('///')) continue
      for (const [kind, pattern] of rules) {
        if (pattern.test(code)) found.add(`${kind} ${rel}`)
      }
    }
  }
}

scan(
  walk(join(root, 'ui', 'src'), (n) => /\.(ts|tsx)$/.test(n) && !/\.test\.tsx?$/.test(n)),
  SOURCE_RULES,
)
scan(walk(join(root, 'src-tauri', 'src'), (n) => n.endsWith('.rs')), RUST_RULES)
scan(walk(join(root, 'cli', 'src'), (n) => n.endsWith('.rs')), RUST_RULES)

// Dependencies that bring their own network stack.
for (const manifest of ['src-tauri/Cargo.toml', 'cli/Cargo.toml']) {
  const cargo = readFileSync(join(root, manifest), 'utf8')
  const depsSection = cargo.split(/^\[dependencies\]/m)[1]?.split(/^\[/m)[0] ?? ''
  for (const line of depsSection.split('\n')) {
    if (NETWORK_CRATES.test(line.trim())) found.add(`network-crate ${manifest} ${line.trim().split(/[\s=]/)[0]}`)
  }
}

// The webview may only talk to itself and the Tauri IPC bridge.
const conf = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'))
for (const key of ['csp', 'devCsp']) {
  const csp = conf.app?.security?.[key]
  if (typeof csp !== 'string') {
    found.add(`csp-missing src-tauri/tauri.conf.json ${key}`)
    continue
  }
  const connect = (csp.match(/connect-src([^;]*)/)?.[1] ?? '').trim().split(/\s+/).filter(Boolean)
  const allowed = key === 'csp' ? ["'self'", 'ipc:', 'http://ipc.localhost'] : ["'self'", 'ipc:', 'http://ipc.localhost', 'ws://127.0.0.1:5173', 'http://127.0.0.1:5173']
  for (const source of connect) if (!allowed.includes(source)) found.add(`csp-connect src-tauri/tauri.conf.json ${source}`)
  if (!connect.length) found.add(`csp-connect-missing src-tauri/tauri.conf.json ${key}`)
}

const allowlistPath = join(root, 'scripts', 'outbound-allowlist.txt')
const allowed = new Map()
for (const raw of readFileSync(allowlistPath, 'utf8').split('\n')) {
  const line = raw.trim()
  if (!line || line.startsWith('#')) continue
  const [entry, ...reason] = line.split('  #')
  allowed.set(entry.trim(), reason.join('#').trim())
}

if (process.argv.includes('--list')) {
  for (const entry of [...found].sort()) console.log(allowed.has(entry) ? `ok       ${entry}` : `UNLISTED ${entry}`)
  process.exit(0)
}

const unlisted = [...found].filter((e) => !allowed.has(e)).sort()
const stale = [...allowed.keys()].filter((e) => !found.has(e)).sort()
const noReason = [...allowed].filter(([, reason]) => !reason).map(([e]) => e)

if (unlisted.length) {
  console.error('Outbound call sites not in scripts/outbound-allowlist.txt:')
  for (const entry of unlisted) console.error(`  ${entry}`)
  console.error('Remove the call, or add it to the allowlist with a reason.')
}
if (stale.length) {
  console.error('Allowlist entries that no longer match anything (remove them):')
  for (const entry of stale) console.error(`  ${entry}`)
}
if (noReason.length) {
  console.error('Allowlist entries without a reason (use "entry  # reason"):')
  for (const entry of noReason) console.error(`  ${entry}`)
}
if (unlisted.length || stale.length || noReason.length) process.exit(1)
console.log(`outbound check passed: ${found.size} known call sites, all listed.`)
