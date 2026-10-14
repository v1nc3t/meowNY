/**
 * Text charts: every function here returns rows of coloured text, nothing else. A page turns the
 * rows into a <pre>. `tone` picks a colour from the page's palette (0, 1, 2 ...); null is the dim
 * background colour. Charts draw with plain numbers; money shown as text elsewhere stays a string.
 */
export type Tone = number | null
export type Seg = { text: string; tone: Tone }
export type Rows = Seg[][]

/** The characters alone, rows joined by newlines. Used for tests and screen-reader text. */
export function rowsToText(rows: Rows): string {
  return rows.map((row) => row.map((seg) => seg.text).join('')).join('\n')
}

/** Joins neighbouring pieces that have the same tone, so a row has as few <span>s as possible. */
export function merge(cells: Seg[]): Seg[] {
  const out: Seg[] = []
  for (const cell of cells) {
    const last = out[out.length - 1]
    if (last && last.tone === cell.tone) last.text += cell.text
    else out.push({ ...cell })
  }
  return out
}
