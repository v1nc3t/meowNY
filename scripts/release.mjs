#!/usr/bin/env node
// Makes a release: sets the new version everywhere, records the command line docs again (they print the version), and
// commits and tags it. The usual way is the Release button in GitHub (Actions, Release, Run workflow), which runs this
// script and then builds and publishes everything. By hand:
//
//   npm run release patch          (or minor, major, or an exact version such as 0.3.0)
//   git push origin main v0.2.1    builds every installer and publishes them (.github/workflows/release.yml)
//
// Run it on Linux, on main as it is on GitHub, with a clean working tree: releases come from main only, and the docs are
// recorded by a test that runs only on Linux (cli/tests/docs.rs).
// No dependencies.

import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(fileURLToPath(import.meta.url), '..', '..')
const at = (...parts) => join(root, ...parts)

function fail(message) {
  console.error(`release: ${message}`)
  process.exit(1)
}

const run = (command, args, options = {}) => execFileSync(command, args, { cwd: root, stdio: 'inherit', ...options })
const output = (command, args) => execFileSync(command, args, { cwd: root, encoding: 'utf8' }).trim()

/** Replaces exactly one match of `pattern` in a file, or stops: a file that changed shape must not be half updated. */
function replaceOnce(relative, pattern, replacement) {
  const path = at(relative)
  const text = readFileSync(path, 'utf8')
  const matches = text.match(new RegExp(pattern.source, pattern.flags.includes('g') ? pattern.flags : pattern.flags + 'g'))
  if (matches?.length !== 1) fail(`${relative}: expected one version to change, found ${matches?.length ?? 0}`)
  writeFileSync(path, text.replace(pattern, replacement))
}

if (process.platform !== 'linux') fail('run it on Linux, where the command line docs are recorded')

const config = JSON.parse(readFileSync(at('src-tauri/tauri.conf.json'), 'utf8'))
const current = config.version

/** `patch`, `minor` or `major` counts on from the current version; anything else must be an exact version. */
function nextVersion(wanted) {
  const [major, minor, patch] = current.split('.').map(Number)
  if (wanted === 'patch') return `${major}.${minor}.${patch + 1}`
  if (wanted === 'minor') return `${major}.${minor + 1}.0`
  if (wanted === 'major') return `${major + 1}.0.0`
  return wanted?.replace(/^v/, '')
}

const next = nextVersion(process.argv[2])
if (!next || !/^\d+\.\d+\.\d+$/.test(next)) fail('say how to count on: npm run release patch (or minor, major, or a version like 0.3.0)')
const newer = next.split('.').map(Number).reduce((r, n, i) => r || n - current.split('.').map(Number)[i], 0) > 0
if (!newer) fail(`${next} is not newer than ${current}`)
if (output('git', ['status', '--porcelain'])) fail('commit or stash your changes first')
if (output('git', ['tag', '--list', `v${next}`])) fail(`the tag v${next} already exists`)
// Releases come from main, and from main as it is on GitHub, so a release never carries commits nobody else has.
if (output('git', ['rev-parse', '--abbrev-ref', 'HEAD']) !== 'main') fail('releases are made from main: git checkout main && git pull')
output('git', ['fetch', '--quiet', 'origin', 'main'])
if (output('git', ['rev-parse', 'HEAD']) !== output('git', ['rev-parse', 'origin/main'])) {
  fail('main here is not main on GitHub: git pull, or push your commits through a pull request first')
}

const v = current.replace(/\./g, '\\.')
replaceOnce('src-tauri/tauri.conf.json', new RegExp(`"version": "${v}"`), `"version": "${next}"`)
replaceOnce('src-tauri/Cargo.toml', new RegExp(`^version = "${v}"`, 'm'), `version = "${next}"`)
replaceOnce('cli/Cargo.toml', new RegExp(`^version = "${v}"`, 'm'), `version = "${next}"`)
for (const [lock, name] of [['src-tauri/Cargo.lock', 'meowny-desktop'], ['cli/Cargo.lock', 'meowny-desktop'], ['cli/Cargo.lock', 'meowny-cli']]) {
  replaceOnce(lock, new RegExp(`(name = "${name}"\\nversion = )"${v}"`), `$1"${next}"`)
}
console.log(`release: ${current} -> ${next}`)

// The docs show real output of `meowny`, and that output has the version in it.
console.log('release: recording the command line docs (this builds the command line, a few minutes the first time)')
run('cargo', ['test', '--test', 'docs'], {
  cwd: at('cli'),
  env: { ...process.env, UPDATE_DOCS: '1', CARGO_TARGET_DIR: at('src-tauri', 'target') },
})
run('node', ['docs/site/build.mjs', '--check'])

run('git', ['add', '-A', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'cli/Cargo.toml', 'cli/Cargo.lock', 'docs/site/commands.json', 'docs/site/examples.json'])
run('git', ['commit', '-q', '-m', `chore: release v${next}`])
run('git', ['tag', '-a', `v${next}`, '-m', `meowNY ${next}`])
console.log(`release: committed and tagged v${next}. To build and publish it:\n\n  git push origin main v${next}\n`)
// The workflow reads the version from this line.
if (process.env.GITHUB_OUTPUT) writeFileSync(process.env.GITHUB_OUTPUT, `version=${next}\n`, { flag: 'a' })
