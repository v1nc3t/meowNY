import assert from 'node:assert/strict'
import { test } from 'node:test'
import { en } from '../i18n/en.ts'
import { Preview } from './dataExport.ts'

// What the Rust side sends for `export_preview` (export.rs, `Preview`).
const preview = {
  transactions: 4,
  firstDate: '2026-01-25',
  lastDate: '2026-03-01',
  income: '3200.00',
  expense: '1885.50',
  categories: 3,
  budgets: 4,
  recurring: 2,
  documents: 6,
  history: 12,
}

test('a preview is read as the app sends it, with the number of documents it lists', () => {
  assert.deepEqual(Preview.parse(preview), preview)
  assert.equal(Preview.parse({ ...preview, firstDate: null, lastDate: null, documents: 0 }).documents, 0)
})

test('a preview without the documents, or with a count that cannot be, is refused', () => {
  for (const bad of [{ ...preview, documents: undefined }, { ...preview, documents: -1 }, { ...preview, documents: 1.5 }, { ...preview, documents: '6' }]) {
    assert.equal(Preview.safeParse(bad).success, false, JSON.stringify(bad))
  }
})

test('the two notes about documents in an export exist', () => {
  assert.ok('export.documentsListed' in en && 'export.documentsColumn' in en)
})
