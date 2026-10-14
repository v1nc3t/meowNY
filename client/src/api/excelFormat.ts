/**
 * How the Excel export should show dates and money, worked out from the person's number and date
 * format setting (a locale such as "de-DE") with the browser's own `Intl`. Excel and LibreOffice choose
 * the digit grouping and decimal mark themselves; this fixes the date order, the date separator and
 * where the currency symbol goes.
 */
export type SheetFormat = {
  dateOrder: 'DMY' | 'MDY' | 'YMD'
  dateSeparator: '/' | '.' | '-' | ' '
  currencySymbol: string
  symbolBefore: boolean
  symbolSpace: boolean
}

/** Used when the locale cannot be formatted: ISO dates and a plain number. */
export const PLAIN_FORMAT: SheetFormat = {
  dateOrder: 'YMD',
  dateSeparator: '-',
  currencySymbol: '',
  symbolBefore: false,
  symbolSpace: false,
}

const SEPARATORS = ['/', '.', '-'] as const

export function sheetFormat(locale: string, currency: string): SheetFormat {
  try {
    // 23 November 2001 has a different number in every position, so the order can be read off.
    const when = new Date(Date.UTC(2001, 10, 23))
    const date = new Intl.DateTimeFormat(locale, { year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'UTC' })
      .formatToParts(when)
    const order = date.filter((p) => p.type === 'day' || p.type === 'month' || p.type === 'year').map((p) => p.type[0])
    const dateOrder = order.join('').toUpperCase()
    if (dateOrder !== 'DMY' && dateOrder !== 'MDY' && dateOrder !== 'YMD') return PLAIN_FORMAT
    const glue = date.find((p) => p.type === 'literal')?.value ?? ''
    const dateSeparator = SEPARATORS.find((s) => glue.includes(s)) ?? (glue.trim() === '' && glue !== '' ? ' ' : '/')

    const money = new Intl.NumberFormat(locale, { style: 'currency', currency, currencyDisplay: 'narrowSymbol' })
      .formatToParts(1234.5)
    const at = money.findIndex((p) => p.type === 'currency')
    if (at < 0) return { dateOrder, dateSeparator, currencySymbol: '', symbolBefore: false, symbolSpace: false }
    const symbolBefore = at < money.findIndex((p) => p.type === 'integer')
    const next = money[symbolBefore ? at + 1 : at - 1]
    const symbolSpace = next?.type === 'literal' && /\s/.test(next.value)
    // The symbol can hold at most 8 characters and no quote; anything odder is left out.
    const symbol = money[at].value
    const usable = symbol.length <= 8 && !/["\p{Cc}]/u.test(symbol)
    return { dateOrder, dateSeparator, currencySymbol: usable ? symbol : '', symbolBefore, symbolSpace }
  } catch {
    return PLAIN_FORMAT
  }
}
