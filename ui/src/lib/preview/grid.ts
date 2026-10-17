/** The letter of a column the way a spreadsheet says it: A to Z, then AA, AB, and so on. */
export function columnName(index: number): string {
  let name = ''
  for (let rest = index + 1; rest > 0; rest = Math.floor((rest - 1) / 26)) name = String.fromCharCode(65 + ((rest - 1) % 26)) + name
  return name
}

const NUMBER = /^-?\d+([.,]\d+)?%?$/

/** Whether a cell is a plain number, which a sheet sets to the right of its cell. A date and a text stay on the left. */
export function looksNumeric(text: string): boolean {
  return NUMBER.test(text)
}
