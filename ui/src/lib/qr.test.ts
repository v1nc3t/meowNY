import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { capacity, dataCodewords, encodeQr, qrPath, totalCodewords, type Level } from './qr.ts'

type Case = { text: string; level: Level; version: number; forcedVersion: boolean; mask: number; forcedMask: boolean; size: number; rows: string[] }
const vectors = JSON.parse(readFileSync(new URL('./qr.vectors.json', import.meta.url), 'utf8')) as { cases: Case[] }

/** The rows of an encoded code as hex, the way the reference file has them. */
function hexRows(modules: boolean[][]): string[] {
  const width = Math.ceil(modules.length / 4)
  return modules.map((row) => BigInt('0b' + row.map((m) => (m ? '1' : '0')).join('')).toString(16).padStart(width, '0'))
}

test('the tables add up: every version and level fills the codewords of the symbol', () => {
  const total = [26, 44, 70, 100, 134, 172, 196, 242, 292, 346]
  for (const level of ['L', 'M', 'Q', 'H'] as const) {
    total.forEach((count, i) => assert.equal(totalCodewords(i + 1, level), count, `version ${i + 1} ${level}`))
  }
  assert.equal(dataCodewords(1, 'L'), 19)
  assert.equal(dataCodewords(10, 'H'), 122)
})

test('the capacity in bytes is the one the specification gives', () => {
  const expected: Record<Level, number[]> = {
    L: [17, 32, 53, 78, 106, 134, 154, 192, 230, 271],
    M: [14, 26, 42, 62, 84, 106, 122, 152, 180, 213],
    Q: [11, 20, 32, 46, 60, 74, 86, 108, 130, 151],
    H: [7, 14, 24, 34, 44, 58, 64, 84, 98, 119],
  }
  for (const level of ['L', 'M', 'Q', 'H'] as const) expected[level].forEach((bytes, i) => assert.equal(capacity(i + 1, level), bytes, `version ${i + 1} ${level}`))
})

test('the reference file is big enough to mean something', () => {
  assert.ok(vectors.cases.length >= 120)
  const versions = new Set(vectors.cases.map((c) => c.version))
  assert.deepEqual([...versions].sort((a, b) => a - b), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
  assert.equal(new Set(vectors.cases.filter((c) => c.forcedMask).map((c) => c.mask)).size, 8)
})

test('every code matches the reference module for module', () => {
  for (const [n, c] of vectors.cases.entries()) {
    const code = encodeQr(c.text, { level: c.level, version: c.forcedVersion ? c.version : undefined, mask: c.forcedMask ? c.mask : undefined })
    const label = `case ${n}: version ${c.version} level ${c.level} mask ${c.mask} (${c.text.length} characters)`
    assert.equal(code.version, c.version, label)
    assert.equal(code.size, c.size, label)
    assert.equal(code.mask, c.mask, `${label}: the mask chosen`)
    assert.deepEqual(hexRows(code.modules), c.rows, label)
  }
})

test('the smallest version that fits is chosen, and a text that does not fit is refused', () => {
  assert.equal(encodeQr('a'.repeat(14), { level: 'M' }).version, 1)
  assert.equal(encodeQr('a'.repeat(15), { level: 'M' }).version, 2)
  assert.equal(encodeQr('a'.repeat(213), { level: 'M' }).version, 10)
  assert.throws(() => encodeQr('a'.repeat(214), { level: 'M' }), RangeError)
  assert.throws(() => encodeQr('a', { version: 11 }), RangeError)
  assert.throws(() => encodeQr('a', { version: 0 }), RangeError)
  assert.throws(() => encodeQr('a'.repeat(100), { level: 'M', version: 3 }), RangeError)
  assert.throws(() => encodeQr('a', { mask: 8 }), RangeError)
})

test('the same text gives the same code, and different text gives a different one', () => {
  const a = encodeQr('MNY1:abc')
  assert.deepEqual(encodeQr('MNY1:abc').modules, a.modules)
  assert.notDeepEqual(encodeQr('MNY1:abd').modules, a.modules)
})

test('text that is not ASCII is encoded as UTF-8 and counted in bytes', () => {
  const code = encodeQr('é'.repeat(7), { level: 'M' }) // 14 bytes: just fits version 1
  assert.equal(code.version, 1)
  assert.equal(encodeQr('é'.repeat(8), { level: 'M' }).version, 2)
})

test('the fixed patterns are where they must be', () => {
  const { modules, size } = encodeQr('MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ')
  // the three finder patterns: a dark 7x7 ring, a light ring, a dark 3x3 centre
  for (const [cx, cy] of [[3, 3], [size - 4, 3], [3, size - 4]]) {
    assert.ok(modules[cy][cx] && modules[cy - 3][cx - 3] && modules[cy + 3][cx + 3] && !modules[cy - 2][cx] && !modules[cy][cx + 2])
  }
  // the timing patterns alternate between the finders
  for (let i = 8; i < size - 8; i++) assert.equal(modules[6][i], i % 2 === 0)
  assert.ok(modules[size - 8][8], 'the module that is always dark')
})

test('the drawing path covers the dark modules and nothing else', () => {
  const { path, size } = qrPath({ modules: [[true, true, false], [false, true, false], [true, false, true]] }, 1)
  assert.equal(size, 5)
  assert.equal(path, 'M1 1h2v1h-2zM2 2h1v1h-1zM1 3h1v1h-1zM3 3h1v1h-1z')
  assert.equal(qrPath({ modules: [[false]] }).path, '')
  // a real code: the area of the rectangles is the number of dark modules
  const code = encodeQr('MNY1:path-check')
  const dark = code.modules.flat().filter(Boolean).length
  const area = [...qrPath(code).path.matchAll(/h(\d+)v1/g)].reduce((sum, m) => sum + Number(m[1]), 0)
  assert.equal(area, dark)
  assert.equal(qrPath(code).size, code.size + 8)
})
