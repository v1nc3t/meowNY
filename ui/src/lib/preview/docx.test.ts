import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readDocx } from './docx.ts'
import { PreviewError } from './errors.ts'
import { makeZip } from './zipFixture.ts'

const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
const docx = (body: string) => makeZip([{ name: 'word/document.xml', data: `<?xml version="1.0"?><w:document ${W}><w:body>${body}</w:body></w:document>`, deflate: true }])
const run = (text: string) => `<w:r><w:t xml:space="preserve">${text}</w:t></w:r>`

test('a heading is also known by its outline level, in any language of its style name', async () => {
  const { blocks } = await readDocx(await docx(`<w:p><w:pPr><w:pStyle w:val="berschrift3"/><w:outlineLvl w:val="2"/></w:pPr>${run('Kapitel')}</w:p>`))
  assert.deepEqual(blocks, [{ type: 'heading', level: 3, text: 'Kapitel' }])
})

test('tabs, line breaks, links and tracked changes are looked through; deleted text and field codes are not shown', async () => {
  const { blocks } = await readDocx(
    await docx(
      '<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t></w:r>' +
        '<w:hyperlink><w:r><w:t> link</w:t></w:r></w:hyperlink>' +
        '<w:ins><w:r><w:t> added</w:t></w:r></w:ins><w:del><w:r><w:delText> removed</w:delText></w:r></w:del>' +
        '<w:r><w:instrText> HYPERLINK "http://x" </w:instrText></w:r></w:p>',
    ),
  )
  assert.deepEqual(blocks, [{ type: 'paragraph', text: 'a\tb\nc link added' }])
})

test('a table is rows of cells, a cell with its paragraphs on lines', async () => {
  const cell = (...lines: string[]) => `<w:tc><w:tcPr><w:tcW w:w="100"/></w:tcPr>${lines.map((line) => `<w:p>${run(line)}</w:p>`).join('')}</w:tc>`
  const { blocks } = await readDocx(await docx(`<w:tbl><w:tr>${cell('item')}${cell('price')}</w:tr><w:tr>${cell('rent', 'May')}${cell('900')}</w:tr></w:tbl>`))
  assert.deepEqual(blocks, [{ type: 'table', rows: [['item', 'price'], ['rent\nMay', '900']] }])
})

test('content controls are looked into, and pictures and text boxes are left out', async () => {
  const { blocks } = await readDocx(
    await docx(
      `<w:sdt><w:sdtPr/><w:sdtContent><w:p>${run('inside a control')}</w:p></w:sdtContent></w:sdt>` +
        `<w:p><w:r><w:drawing><w:t>hidden picture text</w:t></w:drawing></w:r>${run('after the picture')}</w:p>` +
        '<w:p><w:r><w:drawing/></w:r></w:p>',
    ),
  )
  assert.deepEqual(blocks, [
    { type: 'paragraph', text: 'inside a control' },
    { type: 'paragraph', text: 'after the picture' },
  ])
})

test('characters that read one way and are another are taken out of the text', async () => {
  const { blocks } = await readDocx(await docx(`<w:p>${run('pay ‮evil‬​ now\u0007')}</w:p>`))
  assert.deepEqual(blocks, [{ type: 'paragraph', text: 'pay evil now' }])
})

test('a very long file is cut and says so', async () => {
  const paragraph = `<w:p>${run('x'.repeat(10_000))}</w:p>`
  const { blocks, cut } = await readDocx(await docx(paragraph.repeat(60)))
  assert.equal(cut, true)
  assert.ok(blocks.length < 60 && blocks.length > 30)
})

test('a file with no body, or that is not a Word file, is not read', async () => {
  await assert.rejects(readDocx(await makeZip([{ name: 'other.xml', data: '<a/>' }])), PreviewError)
  await assert.rejects(readDocx(await makeZip([{ name: 'word/document.xml', data: '<w:document/>' }])), PreviewError)
  await assert.rejects(readDocx(new TextEncoder().encode('just text')), PreviewError)
})
