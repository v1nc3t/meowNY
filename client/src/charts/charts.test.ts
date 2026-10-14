import assert from 'node:assert/strict'
import { test } from 'node:test'
import { BrailleCanvas, columns, donut, lineChart, meter, rowsToText, sparkline } from './index.ts'

test('each dot of a braille cell is its own bit', () => {
  const canvas = new BrailleCanvas(1, 1)
  assert.equal(canvas.cell(0, 0).text, '⠀')
  canvas.set(0, 0)
  assert.equal(canvas.cell(0, 0).text, '⠁')
  canvas.set(1, 3)
  assert.equal(canvas.cell(0, 0).text, '⢁')
  for (let x = 0; x < 2; x++) for (let y = 0; y < 4; y++) canvas.set(x, y)
  assert.equal(canvas.cell(0, 0).text, '⣿')
})

test('dots outside the canvas are ignored and a cell takes its latest tone', () => {
  const canvas = new BrailleCanvas(1, 1)
  canvas.set(-1, 0)
  canvas.set(2, 0)
  canvas.set(0, 4)
  assert.equal(canvas.cell(0, 0).text, '⠀')
  canvas.set(0, 0, 1)
  canvas.set(1, 0, 2)
  assert.equal(canvas.cell(0, 0).tone, 2)
})

test('a line across a canvas lights one dot per column', () => {
  const canvas = new BrailleCanvas(2, 1)
  canvas.line(0, 0, 3, 3)
  assert.equal(rowsToText(canvas.rows()), '⠑⢄')
})

test('a rising line starts at the bottom left and ends at the top right', () => {
  const rows = lineChart([{ values: [0, 5, 10] }], { width: 6, height: 2 })
  assert.equal(rows.length, 2)
  assert.equal(rows[0][0].text.length + (rows[0][1]?.text.length ?? 0), 6)
  const text = rowsToText(rows).split('\n')
  assert.notEqual(text[1][0], '⠀', 'bottom-left has a dot')
  assert.equal(text[0][0], '⠀', 'top-left is empty')
  assert.notEqual(text[0][5], '⠀', 'top-right has a dot')
})

test('gaps in a series are left blank and series get their own tones', () => {
  const rows = lineChart([{ values: [1, null, 1] }, { values: [2, 2, 2] }], { width: 4, height: 2, min: 0, max: 2 })
  // Series two is a flat line along the top; series one is two lone dots lower down with nothing between.
  assert.equal(rowsToText(rows), '⠉⠉⠉⠉\n⠁⠀⠀⠈')
  assert.deepEqual(rows[0].map((seg) => seg.tone), [1])
  assert.equal(rows[1][0].tone, 0)
})

test('a meter fills in eighths and draws the rest dim', () => {
  assert.equal(rowsToText([meter(0.5, 10)]), '█████░░░░░')
  assert.equal(rowsToText([meter(0, 4)]), '░░░░')
  assert.equal(rowsToText([meter(1, 4)]), '████')
  assert.equal(rowsToText([meter(0.3125, 4)]), '█▎░░')
  assert.equal(rowsToText([meter(2, 4)]), '████', 'over a hundred percent is full')
  assert.equal(rowsToText([meter(Number.NaN, 3)]), '░░░')
  assert.deepEqual(meter(0.5, 4, 3).map((s) => s.tone), [3, null])
})

test('a sparkline scales to the largest value', () => {
  assert.equal(rowsToText([sparkline([0, 1, 2, 4, 8])]), ' ▁▂▄█')
  assert.equal(rowsToText([sparkline([0, 0])]), '  ')
  assert.equal(rowsToText([sparkline([5], 0, 10)]), '▄')
})

test('columns stack from the bottom and groups are separated by a blank', () => {
  // Two groups of two series, two rows tall, scale 0 to 8: the tallest column fills both rows.
  const rows = columns([{ values: [8, 4] }, { values: [0, 8] }], 2)
  assert.equal(rowsToText(rows), '█   █\n██  █')
  assert.deepEqual(rows[1].map((seg) => seg.tone), [0, 1, null, 0, 1])
  assert.equal(rowsToText(columns([{ values: [3] }], 1, 8)), '▃')
  assert.equal(rowsToText(columns([{ values: [0] }], 1)), ' ')
})

test('the donut has the requested size and the slices fill the ring in order', () => {
  const rows = donut([{ value: 1 }, { value: 1 }], { size: 8 })
  assert.equal(rows.length, 8)
  assert.ok(rows.every((row) => row.reduce((n, seg) => n + seg.text.length, 0) === 16))
  const topRight = rows[0].flatMap((seg) => [...seg.text].map(() => seg.tone))
  assert.equal(topRight[9], 0, 'clockwise from twelve: the first slice starts at the top right')
  const bottomLeft = rows[7].flatMap((seg) => [...seg.text].map(() => seg.tone))
  assert.equal(bottomLeft[6], 1, 'the second slice ends at the left')
})

test('a donut with nothing in it is a dim ring, and bad values count as zero', () => {
  for (const slices of [[], [{ value: 0 }], [{ value: -3 }, { value: Number.NaN }]]) {
    const rows = donut(slices, { size: 6 })
    assert.ok(rows.flat().every((seg) => seg.tone === null))
    assert.match(rowsToText(rows), /[⠁-⣿]/, 'the ring is still drawn')
  }
})

test('donut text goes in the middle, cut to the hole', () => {
  const rows = donut([{ value: 1 }], { size: 8, center: '89%' })
  assert.ok(rowsToText(rows).split('\n')[4].includes('89%'))
  const cut = donut([{ value: 1 }], { size: 8, center: 'abcdefghijklmnop' })
  const middle = rowsToText(cut).split('\n')[4]
  assert.ok(middle.includes('abcdefghi') && !middle.includes('j'), 'cut to the 9 characters the hole holds')
})
