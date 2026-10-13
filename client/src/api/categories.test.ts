import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Category, cleanCategoryName } from './categories.ts'

test('category names are trimmed and bounded by characters', () => {
  assert.equal(cleanCategoryName('  Food '), 'Food')
  assert.equal(cleanCategoryName(''), null)
  assert.equal(cleanCategoryName('   '), null)
  assert.equal(cleanCategoryName('x'.repeat(51)), null)
  assert.equal(cleanCategoryName('é'.repeat(50)), 'é'.repeat(50))
  assert.equal(cleanCategoryName('a\nb'), null)
})

test('a category answer must have a string id and a known type', () => {
  assert.ok(Category.safeParse({ id: '3', name: 'Food', type: 'EXPENSE' }).success)
  assert.ok(!Category.safeParse({ id: 3, name: 'Food', type: 'EXPENSE' }).success)
  assert.ok(!Category.safeParse({ id: '0', name: 'Food', type: 'EXPENSE' }).success)
  assert.ok(!Category.safeParse({ id: '3', name: 'Food', type: 'OTHER' }).success)
})
