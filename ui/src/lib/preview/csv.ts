import { clean } from './clean'
import { MAX_COLUMNS, MAX_ROWS } from './xlsx'

/**
 * A CSV file as rows of cells (RFC 4180: a cell may be quoted, and a quoted cell may hold the separator, a line break and a doubled
 * quote). The separator is the comma, or the semicolon when the first line has more of those, which is how many programs write it
 * where the comma is the decimal mark. Only the first 1,000 rows and 40 columns are kept; `cut` says whether there was more.
 */
export function parseCsv(text: string): { rows: string[][]; cut: boolean } {
  const firstLine = text.slice(0, text.search(/\r|\n|$/))
  const separator = count(firstLine, ';') > count(firstLine, ',') ? ';' : ','
  const rows: string[][] = []
  let row: string[] = []
  let cell = ''
  let quoted = false
  let cut = false
  let started = false

  function endCell(): void {
    if (row.length < MAX_COLUMNS) row.push(clean(cell))
    else cut = true
    cell = ''
  }

  function endRow(): boolean {
    endCell()
    if (rows.length >= MAX_ROWS) {
      cut = true
      return false
    }
    rows.push(row)
    row = []
    started = false
    return true
  }

  for (let at = 0; at < text.length; at += 1) {
    const char = text[at] as string
    if (quoted) {
      if (char === '"' && text[at + 1] === '"') {
        cell += '"'
        at += 1
      } else if (char === '"') {
        quoted = false
      } else {
        cell += char
      }
    } else if (char === '"' && cell === '') {
      quoted = true
      started = true
    } else if (char === separator) {
      endCell()
      started = true
    } else if (char === '\n' || char === '\r') {
      if (char === '\r' && text[at + 1] === '\n') at += 1
      if (!endRow()) return { rows, cut }
    } else {
      cell += char
      started = true
    }
  }
  if (started || cell !== '' || row.length > 0) endRow()
  return { rows, cut }
}

function count(text: string, char: string): number {
  return text.split(char).length - 1
}
