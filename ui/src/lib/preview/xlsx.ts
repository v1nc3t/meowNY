import { clean } from './clean'
import { PreviewError } from './errors'
import { child, children, parseXml, textOf, type XmlNode } from './xml'
import { openZip, type Zip } from './zip'

/** One sheet of an Excel file as the preview shows it: rows of cells as text. `cut` is whether rows or columns were left out. */
export type Sheet = { name: string; rows: string[][]; cut: boolean }
export type XlsxContent = { sheets: Sheet[] }

export const MAX_ROWS = 1000
export const MAX_COLUMNS = 40
const MAX_SHEETS = 40
const MAX_SHARED_STRINGS = 1_000_000

async function xml(zip: Zip, name: string): Promise<XmlNode | null> {
  const part = await zip.read(name)
  return part ? parseXml(new TextDecoder('utf-8').decode(part)) : null
}

/** All the text inside an element, but not a phonetic guide (the small reading aid Japanese text can carry above it). */
function textInside(node: XmlNode): string {
  let out = ''
  for (const kid of node.kids) {
    if (typeof kid === 'string') {
      if (node.name === 't') out += kid
    } else if (kid.name !== 'rPh' && kid.name !== 'phoneticPr') {
      out += textInside(kid)
    }
  }
  return out
}

/** The numbers Excel gives to formats that are dates or times, before a file names its own (ids from 164 up). */
const BUILT_IN_DATE_FORMATS = new Set([14, 15, 16, 17, 18, 19, 20, 21, 22, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 45, 46, 47, 50, 51, 52, 53, 54, 55, 56, 57, 58])

/** Whether a format code of the file's own shows a date or a time: it has a day, month, year, hour or second outside quotes and brackets. */
export function isDateCode(code: string): boolean {
  const bare = code.replace(/"[^"]*"/g, '').replace(/\[[^\]]*\]/g, '').replace(/\\./g, '').replace(/[_*]./g, '')
  return /[ymdhs]/i.test(bare)
}

/** Which style numbers (the `s` of a cell) are dates. */
function dateStyles(styles: XmlNode | null): boolean[] {
  if (!styles) return []
  const codes = new Map<number, string>()
  for (const format of children(child(styles, 'numFmts') ?? styles, 'numFmt')) codes.set(Number(format.attrs.numFmtId), format.attrs.formatCode ?? '')
  const formats = child(styles, 'cellXfs')
  return children(formats ?? styles, 'xf').map((xf) => {
    const id = Number(xf.attrs.numFmtId)
    return BUILT_IN_DATE_FORMATS.has(id) || (id >= 164 && isDateCode(codes.get(id) ?? ''))
  })
}

/** A date or time that Excel keeps as days since a day (and a part of a day). `null` if it is out of the years 1 to 9999. */
export function excelDate(serial: number, date1904: boolean): string | null {
  if (!Number.isFinite(serial) || serial < 0 || serial >= 2_958_466) return null
  const day = Math.floor(serial)
  const milliseconds = Math.round((serial - day) * 86_400_000)
  // Excel counts a 29th of February 1900 that never was, so before March 1900 its days are one behind
  const base = date1904 ? Date.UTC(1904, 0, 1) : day < 60 ? Date.UTC(1899, 11, 31) : Date.UTC(1899, 11, 30)
  const at = new Date(base + day * 86_400_000 + milliseconds).toISOString()
  const time = at.slice(11, 16)
  if (day === 0) return time
  return milliseconds === 0 ? at.slice(0, 10) : `${at.slice(0, 10)} ${time}`
}

/** A number as a person reads it: what Excel kept as 0.30000000000000004 is 0.3. */
function numberText(text: string): string {
  const value = Number(text)
  if (!Number.isFinite(value)) return text
  return Number.isInteger(value) ? String(value) : String(Number(value.toPrecision(12)))
}

function columnOf(reference: string): number {
  let column = 0
  for (const char of reference) {
    const code = char.charCodeAt(0)
    if (code < 65 || code > 90) break
    column = column * 26 + (code - 64)
  }
  return column - 1
}

/** The part of a sheet's name in the rels file, relative to `xl/`. */
function partOf(target: string): string {
  return target.startsWith('/') ? target.slice(1) : `xl/${target}`
}

function sheetRows(sheet: XmlNode, shared: string[], dates: boolean[], date1904: boolean): { rows: string[][]; cut: boolean } {
  const data = child(sheet, 'sheetData')
  const rows: string[][] = []
  let cut = false
  let nextRow = 0
  for (const row of children(data ?? sheet, 'row')) {
    const number = row.attrs.r !== undefined && /^[0-9]+$/.test(row.attrs.r) ? Number(row.attrs.r) - 1 : nextRow
    nextRow = number + 1
    if (number >= MAX_ROWS) {
      cut = true
      break
    }
    const cells: string[] = []
    let nextColumn = 0
    for (const cell of children(row, 'c')) {
      const column = cell.attrs.r !== undefined ? columnOf(cell.attrs.r) : nextColumn
      nextColumn = column + 1
      if (column < 0) continue
      if (column >= MAX_COLUMNS) {
        cut = true
        continue
      }
      while (cells.length < column) cells.push('')
      cells[column] = cellText(cell, shared, dates, date1904)
    }
    while (rows.length < number) rows.push([])
    rows[number] = cells
  }
  while (rows.length > 0 && (rows[rows.length - 1] ?? []).every((cell) => cell === '')) rows.pop()
  return { rows, cut }
}

function cellText(cell: XmlNode, shared: string[], dates: boolean[], date1904: boolean): string {
  const kind = cell.attrs.t
  const valueNode = child(cell, 'v')
  const value = valueNode ? textOf(valueNode) : ''
  if (kind === 'inlineStr') return clean(textInside(child(cell, 'is') ?? cell))
  if (value === '') return ''
  if (kind === 's') return shared[Number(value)] ?? ''
  if (kind === 'b') return value === '1' ? 'TRUE' : 'FALSE'
  if (kind === 'str' || kind === 'e' || kind === 'd') return clean(value)
  const style = cell.attrs.s !== undefined ? Number(cell.attrs.s) : -1
  if (dates[style]) return excelDate(Number(value), date1904) ?? numberText(value)
  return numberText(value)
}

/**
 * The cells of an Excel file (.xlsx): each sheet's values, as text, the first 1,000 rows and 40 columns of it. A formula shows
 * the value it last had. Dates are shown as dates. Not shown: formulas, formatting (colours, currency signs, widths), merged
 * cells, charts, pictures and hidden sheets. The file is only read, and nothing in it is calculated.
 */
export async function readXlsx(bytes: Uint8Array): Promise<XlsxContent> {
  const zip = openZip(bytes)
  const workbook = await xml(zip, 'xl/workbook.xml')
  if (!workbook) throw new PreviewError('unreadable')
  const date1904 = ['1', 'true'].includes(child(workbook, 'workbookPr')?.attrs.date1904 ?? '')
  const targets = new Map<string, string>()
  const rels = await xml(zip, 'xl/_rels/workbook.xml.rels')
  for (const rel of children(rels ?? workbook, 'Relationship')) targets.set(rel.attrs.Id ?? '', rel.attrs.Target ?? '')

  const shared: string[] = []
  const strings = await xml(zip, 'xl/sharedStrings.xml')
  for (const item of children(strings ?? workbook, 'si').slice(0, MAX_SHARED_STRINGS)) shared.push(clean(textInside(item)))
  const dates = dateStyles(await xml(zip, 'xl/styles.xml'))

  const listed = children(child(workbook, 'sheets') ?? workbook, 'sheet')
  const sheets: Sheet[] = []
  let position = 0
  for (const entry of listed.slice(0, MAX_SHEETS)) {
    position += 1
    if (entry.attrs.state === 'hidden' || entry.attrs.state === 'veryHidden') continue
    const target = targets.get(entry.attrs.id ?? '')
    const sheet = await xml(zip, target ? partOf(target) : `xl/worksheets/sheet${position}.xml`)
    if (!sheet) continue
    sheets.push({ name: clean(entry.attrs.name ?? '') || String(position), ...sheetRows(sheet, shared, dates, date1904) })
  }
  if (sheets.length === 0) throw new PreviewError('unreadable')
  return { sheets }
}
