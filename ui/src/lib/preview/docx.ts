import { clean, MAX_TEXT_CHARS } from './clean'
import { PreviewError } from './errors'
import { child, children, parseXml, textOf, type XmlNode } from './xml'
import { openZip } from './zip'

/** What the preview shows of a Word file: its text in order, with headings, list items and tables. */
export type DocBlock =
  | { type: 'heading'; level: number; text: string }
  | { type: 'paragraph'; text: string }
  | { type: 'item'; text: string }
  | { type: 'table'; rows: string[][] }

export type DocxContent = { blocks: DocBlock[]; cut: boolean }

const MAX_TABLE_ROWS = 2000
const MAX_TABLE_COLUMNS = 40

/** Parts of a run that hold no text to show: deleted text, field codes, pictures (the preview does not draw pictures). */
const SKIPPED = new Set(['pPr', 'rPr', 'del', 'delText', 'instrText', 'drawing', 'pict', 'object', 'AlternateContent', 'footnoteReference', 'endnoteReference', 'commentReference'])

function collect(node: XmlNode, parts: string[]): void {
  for (const kid of children(node)) {
    if (kid.name === 't') parts.push(textOf(kid))
    else if (kid.name === 'tab') parts.push('\t')
    else if (kid.name === 'br' || kid.name === 'cr') parts.push('\n')
    else if (kid.name === 'noBreakHyphen') parts.push('-')
    else if (!SKIPPED.has(kid.name)) collect(kid, parts)
  }
}

/** The text of one paragraph, with what holds it (runs, links, content controls, tracked insertions) looked through. */
function paragraphText(paragraph: XmlNode): string {
  const parts: string[] = []
  collect(paragraph, parts)
  return clean(parts.join('')).trim()
}

/** 1 to 9 for a heading or a title, 0 for anything else. The level comes from the style's name, or from the outline level. */
function headingLevel(paragraph: XmlNode): number {
  const properties = child(paragraph, 'pPr')
  if (!properties) return 0
  const style = child(properties, 'pStyle')?.attrs.val ?? ''
  const named = /^heading\s*([1-9])/i.exec(style)
  if (named) return Number(named[1])
  if (/^(title|subtitle)$/i.test(style)) return 1
  const outline = child(properties, 'outlineLvl')?.attrs.val
  return outline !== undefined && /^[0-8]$/.test(outline) ? Number(outline) + 1 : 0
}

function isItem(paragraph: XmlNode): boolean {
  const properties = child(paragraph, 'pPr')
  const numbering = properties ? child(properties, 'numPr') : undefined
  return numbering !== undefined && child(numbering, 'numId')?.attrs.val !== '0'
}

/** Every paragraph inside a node, a nested table's too, in order. */
function paragraphsIn(node: XmlNode, into: string[]): void {
  for (const kid of children(node)) {
    if (kid.name === 'p') {
      const text = paragraphText(kid)
      if (text !== '') into.push(text)
    } else if (kid.name !== 'tcPr') {
      paragraphsIn(kid, into)
    }
  }
}

function tableRows(table: XmlNode): string[][] {
  const rows: string[][] = []
  for (const row of children(table, 'tr').slice(0, MAX_TABLE_ROWS)) {
    const cells: string[] = []
    for (const cell of children(row, 'tc').slice(0, MAX_TABLE_COLUMNS)) {
      const lines: string[] = []
      paragraphsIn(cell, lines)
      cells.push(lines.join('\n'))
    }
    rows.push(cells)
  }
  return rows
}

/**
 * The text of a Word file (.docx): paragraphs, headings, list items and tables. Not shown: pictures, text boxes, headers and
 * footers, notes, comments, the look of anything. The file is only read. It is the body of the document, `word/document.xml`, that
 * is looked at; a file with none is not a Word file.
 */
export async function readDocx(bytes: Uint8Array): Promise<DocxContent> {
  const zip = openZip(bytes)
  const part = await zip.read('word/document.xml')
  if (!part) throw new PreviewError('unreadable')
  const body = child(parseXml(new TextDecoder('utf-8').decode(part)), 'body')
  if (!body) throw new PreviewError('unreadable')
  const blocks: DocBlock[] = []
  let chars = 0
  let cut = false

  function add(block: DocBlock): void {
    chars += block.type === 'table' ? block.rows.reduce((sum, row) => sum + row.reduce((inner, cell) => inner + cell.length, 0), 0) : block.text.length
    blocks.push(block)
    if (chars > MAX_TEXT_CHARS) cut = true
  }

  function walk(container: XmlNode): void {
    for (const node of children(container)) {
      if (cut) return
      if (node.name === 'p') {
        const text = paragraphText(node)
        if (text === '') continue
        const level = headingLevel(node)
        if (level > 0) add({ type: 'heading', level, text })
        else add({ type: isItem(node) ? 'item' : 'paragraph', text })
      } else if (node.name === 'tbl') {
        const rows = tableRows(node)
        if (rows.length > 0) add({ type: 'table', rows })
      } else if (node.name === 'sdt') {
        const content = child(node, 'sdtContent')
        if (content) walk(content)
      } else if (node.name === 'ins' || node.name === 'customXml' || node.name === 'sdtContent') {
        walk(node)
      }
    }
  }

  walk(body)
  return { blocks, cut }
}
