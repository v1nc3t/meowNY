import { memoByKey } from './cache'
const transactionAmount = /^(?:0\.(?:0[1-9]|[1-9]\d)|[1-9]\d{0,9}\.\d{2})$/
const budgetAmount = /^(?:0\.\d{2}|[1-9]\d{0,9}\.\d{2})$/

/** `10` and `10.5` become `10.00` and `10.50`. Empty, negative, and zero (unless allowed) return null. */
export function amountString(raw: string, allowZero: boolean): string | null {
  const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(raw.trim())
  const whole = match?.[1]
  if (!whole || whole.length > 10 || (whole.length > 1 && whole.startsWith('0'))) return null
  const amount = `${whole}.${(match[2] ?? '').padEnd(2, '0')}`
  return (allowZero ? budgetAmount : transactionAmount).test(amount) ? amount : null
}

const currencyFormatter = memoByKey(
  (locale: string, currency: string) => `${locale}|${currency}`,
  (locale: string, currency: string) => {
    try {
      return new Intl.NumberFormat(locale, { style: 'currency', currency })
    } catch {
      return null
    }
  },
)

export function formatMoney(amount: string, currency: string, locale: string): string {
  const value = Number(amount)
  if (!Number.isFinite(value)) return amount
  return currencyFormatter(locale, currency)?.format(value) ?? amount
}

/** An exchange rate as an exact fraction: "1.08" is 108/100. Same rules as the Rust side. */
export function parseRate(text: string): { numerator: bigint; denominator: bigint } | null {
  const match = /^(0|[1-9]\d{0,8})(?:\.(\d{1,8}))?$/.exec(text)
  if (!match) return null
  const fraction = match[2] ?? ''
  const numerator = BigInt(`${match[1]}${fraction}`)
  if (numerator === 0n) return null
  return { numerator, denominator: 10n ** BigInt(fraction.length) }
}

/**
 * Converts a "12.50" amount at a typed rate, rounding half up to the cent, exactly like the backend.
 * Used for the preview before the change is made. Returns null for a bad amount or rate.
 */
export function convertAmount(amount: string, rate: string): string | null {
  const parsed = parseRate(rate)
  const match = /^(\d{1,10})\.(\d{2})$/.exec(amount)
  if (!parsed || !match) return null
  const minor = BigInt(`${match[1]}${match[2]}`)
  const converted = (minor * parsed.numerator * 2n + parsed.denominator) / (2n * parsed.denominator)
  const text = converted.toString().padStart(3, '0')
  return `${text.slice(0, -2)}.${text.slice(-2)}`
}

const dateFormatter = memoByKey(
  (locale: string) => locale,
  (locale: string) => {
    try {
      return new Intl.DateTimeFormat(locale, { day: '2-digit', month: '2-digit', year: 'numeric', timeZone: 'UTC' })
    } catch {
      return null
    }
  },
)

/** "2026-10-12" written the way the locale writes dates. The calendar day never shifts with the time zone. */
export function formatDate(isoDate: string, locale: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate)
  if (!match) return isoDate
  return (
    dateFormatter(locale)?.format(new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])))) ?? isoDate
  )
}
