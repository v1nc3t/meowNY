import assert from 'node:assert/strict'
import { readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

// Windows and macOS do not tell `Decrypting.tsx` from `decrypting.ts`: there an import of one finds the other and the build stops.
// Linux keeps them apart, so only the slower Windows and macOS jobs would find it; this test finds it on every run.
test('no two files of the page differ only in capital letters', () => {
  const root = fileURLToPath(new URL('.', import.meta.url))
  const seen = new Map<string, string>()
  for (const path of readdirSync(root, { recursive: true, encoding: 'utf8' })) {
    // An import leaves out the extension, so `a.ts` and `A.tsx` clash too.
    const key = path.toLowerCase().replace(/\.(tsx?|css)$/, '')
    const other = seen.get(key)
    assert.equal(other, undefined, `${path} and ${other} differ only in capital letters`)
    seen.set(key, path)
  }
})
