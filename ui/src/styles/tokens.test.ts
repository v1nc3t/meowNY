import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const css = readFileSync(new URL('./tokens.css', import.meta.url), 'utf8')

/** The declarations between the braces of the block that starts at `opening`, as one list of lines. */
function declarations(opening: string): string[] {
  const start = css.indexOf(opening)
  assert.ok(start >= 0, `${opening} is missing`)
  const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start))
  return body
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.startsWith('--') || line.startsWith('color-scheme'))
}

test('the dark colours are the same for a chosen dark theme and for following the system', () => {
  const chosen = declarations(":root[data-theme='dark']")
  const followed = declarations(":root:not([data-theme='light'])")
  assert.ok(chosen.length > 30)
  assert.deepEqual(followed, chosen)
})

test('the light colours and the dark colours name the same tokens', () => {
  const name = (line: string) => line.split(':')[0]
  const light = declarations(':root {').filter((line) => /^--[a-z0-9-]+: (#|rgb)/.test(line) || line.startsWith('color-scheme'))
  const dark = declarations(":root[data-theme='dark']")
  const colours = new Set(light.map(name))
  for (const line of dark) assert.ok(colours.has(name(line)), `${name(line)} has no light value`)
})
