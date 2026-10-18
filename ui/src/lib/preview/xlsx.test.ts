import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PreviewError } from './errors.ts'
import { excelDate, MAX_COLUMNS, MAX_ROWS, readXlsx } from './xlsx.ts'
import { makeZip, type FixtureEntry } from './zipFixture.ts'

const NS = 'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

async function workbook(sheets: { name: string; xml: string; state?: string }[], extra: FixtureEntry[] = [], workbookPr = ''): Promise<Uint8Array> {
  const list = sheets.map((sheet, at) => `<sheet name="${sheet.name}" sheetId="${at + 1}" r:id="rId${at + 1}"${sheet.state ? ` state="${sheet.state}"` : ''}/>`).join('')
  const rels = sheets.map((_, at) => `<Relationship Id="rId${at + 1}" Type="x" Target="worksheets/sheet${at + 1}.xml"/>`).join('')
  return makeZip([
    { name: 'xl/workbook.xml', data: `<workbook ${NS}>${workbookPr}<sheets>${list}</sheets></workbook>`, deflate: true },
    { name: 'xl/_rels/workbook.xml.rels', data: `<Relationships>${rels}</Relationships>` },
    ...sheets.map((sheet, at) => ({ name: `xl/worksheets/sheet${at + 1}.xml`, data: `<worksheet ${NS}><sheetData>${sheet.xml}</sheetData></worksheet>`, deflate: true })),
    ...extra,
  ])
}

test('a gap in the cells or in the rows stays a gap', async () => {
  const { sheets } = await readXlsx(await workbook([{ name: 'S', xml: '<row r="2"><c r="C2" t="str"><v>x</v></c></row><row r="4"><c r="A4" t="str"><v>y</v></c></row>' }]))
  assert.deepEqual(sheets[0]?.rows, [[], ['', '', 'x'], [], ['y']])
})

test('a cell of a date style shows a date, and the other kinds of number do not', async () => {
  const styles = '<styleSheet><numFmts><numFmt numFmtId="164" formatCode="yyyy\\-mm\\-dd hh:mm"/><numFmt numFmtId="165" formatCode="0.00&quot;d&quot;"/></numFmts><cellXfs><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="164"/><xf numFmtId="165"/></cellXfs></styleSheet>'
  const { sheets } = await readXlsx(
    await workbook(
      [{ name: 'S', xml: '<row r="1"><c r="A1" s="1"><v>45292</v></c><c r="B1" s="2"><v>45292.75</v></c><c r="C1" s="3"><v>45292</v></c><c r="D1" s="0"><v>45292</v></c></row>' }],
      [{ name: 'xl/styles.xml', data: styles }],
    ),
  )
  assert.deepEqual(sheets[0]?.rows, [['2024-01-01', '2024-01-01 18:00', '45292', '45292']])
})

test('dates are counted from the right day in either system, and a value out of range stays a number', () => {
  assert.equal(excelDate(1, false), '1900-01-01')
  assert.equal(excelDate(59, false), '1900-02-28')
  assert.equal(excelDate(61, false), '1900-03-01')
  assert.equal(excelDate(44927, false), '2023-01-01')
  assert.equal(excelDate(43465, true), '2023-01-01')
  assert.equal(excelDate(0.5, false), '12:00')
  assert.equal(excelDate(-1, false), null)
  assert.equal(excelDate(3_000_000, false), null)
  assert.equal(excelDate(Number.NaN, false), null)
})

test('the 1904 date system is read from the workbook', async () => {
  const styles = '<styleSheet><cellXfs><xf numFmtId="14"/></cellXfs></styleSheet>'
  const { sheets } = await readXlsx(await workbook([{ name: 'S', xml: '<row r="1"><c r="A1" s="0"><v>43465</v></c></row>' }], [{ name: 'xl/styles.xml', data: styles }], '<workbookPr date1904="1"/>'))
  assert.deepEqual(sheets[0]?.rows, [['2023-01-01']])
})

test('sheets keep their order and names, and a hidden one is not shown', async () => {
  const one = '<row r="1"><c r="A1" t="str"><v>one</v></c></row>'
  const { sheets } = await readXlsx(await workbook([{ name: 'First', xml: one }, { name: 'Secret', xml: one, state: 'hidden' }, { name: 'Third', xml: one }]))
  assert.deepEqual(sheets.map((sheet) => sheet.name), ['First', 'Third'])
})

test('more rows or columns than the preview shows are left out, and the sheet says so', async () => {
  const rows = Array.from({ length: MAX_ROWS + 50 }, (_, at) => `<row r="${at + 1}"><c r="A${at + 1}"><v>${at}</v></c></row>`).join('')
  const tall = (await readXlsx(await workbook([{ name: 'S', xml: rows }]))).sheets[0]!
  assert.equal(tall.rows.length, MAX_ROWS)
  assert.equal(tall.cut, true)

  const wide = '<row r="1">' + Array.from({ length: 45 }, (_, at) => `<c t="str"><v>${at}</v></c>`).join('') + '</row>'
  const sheet = (await readXlsx(await workbook([{ name: 'S', xml: wide }]))).sheets[0]!
  assert.equal(sheet.rows[0]?.length, MAX_COLUMNS)
  assert.equal(sheet.cut, true)
})

test('what is not a workbook is not read', async () => {
  await assert.rejects(readXlsx(await makeZip([{ name: 'other', data: 'x' }])), PreviewError)
  await assert.rejects(readXlsx(await makeZip([{ name: 'xl/workbook.xml', data: `<workbook ${NS}><sheets/></workbook>` }])), PreviewError)
  await assert.rejects(readXlsx(new Uint8Array(10)), PreviewError)
})

test('text in a cell is cleaned like all the other text of a file', async () => {
  const { sheets } = await readXlsx(await workbook([{ name: 'S‮', xml: '<row r="1"><c r="A1" t="inlineStr"><is><t>a‮b​c</t></is></c></row>' }]))
  assert.equal(sheets[0]?.name, 'S')
  assert.deepEqual(sheets[0]?.rows, [['abc']])
})
