import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseCsv } from './csv.ts'
import { MAX_COLUMNS, MAX_ROWS } from './xlsx.ts'

test('rows and cells are split at the commas and the line ends, whichever way a line ends', () => {
  assert.deepEqual(parseCsv('a,b\n1,2\r\n3,4\r5,6').rows, [['a', 'b'], ['1', '2'], ['3', '4'], ['5', '6']])
  assert.deepEqual(parseCsv('a,b\n').rows, [['a', 'b']], 'a last line end does not make a row')
  assert.deepEqual(parseCsv('').rows, [])
})

test('a quoted cell may hold the separator, a line break and a doubled quote', () => {
  assert.deepEqual(parseCsv('name,note\n"Smith, J","said ""hi""\nthen left"\n').rows, [['name', 'note'], ['Smith, J', 'said "hi"\nthen left']])
})

test('a semicolon is the separator when the first line has more of those', () => {
  assert.deepEqual(parseCsv('item;price\nrent;900,50').rows, [['item', 'price'], ['rent', '900,50']])
})

test('empty cells stay empty', () => {
  assert.deepEqual(parseCsv('a,,c\n,,').rows, [['a', '', 'c'], ['', '', '']])
})

test('rows and columns beyond the limits are left out, and it is said', () => {
  const tall = parseCsv(Array.from({ length: MAX_ROWS + 10 }, (_, at) => String(at)).join('\n'))
  assert.equal(tall.rows.length, MAX_ROWS)
  assert.equal(tall.cut, true)
  const wide = parseCsv(Array.from({ length: MAX_COLUMNS + 3 }, (_, at) => String(at)).join(','))
  assert.equal(wide.rows[0]?.length, MAX_COLUMNS)
  assert.equal(wide.cut, true)
  assert.equal(parseCsv('a,b').cut, false)
})

test('characters that hide or reverse text are taken out of the cells', () => {
  assert.deepEqual(parseCsv('a‮b,c​d').rows, [['ab', 'cd']])
})
