import { memoByKey } from './cache'
const transactionAmount = /^(?:0\.(?:0[1-9]|[1-9]\d)|[1-9]\d{0,9}\.\d{2})$/
const budgetAmount = /^(?:0\.\d{2}|[1-9]\d{0,9}\.\d{2})$/

/** `10`, `10.5` and `10,5` (the number keypad of many phones types a comma) become `10.00`, `10.50` and `10.50`. Empty, negative, and zero (unless allowed) return null. */
export function amountString(raw: string, allowZero: boolean): string | null {
  const match = /^(\d+)(?:[.,](\d{1,2}))?$/.exec(raw.trim())
  const whole = match?.[1]
  if (!whole || whole.length > 10 || (whole.length > 1 && whole.startsWith('0'))) return null
  const amount = `${whole}.${(match[2] ?? '').padEnd(2, '0')}`
  return (allowZero ? budgetAmount : transactionAmount).test(amount) ? amount : null
}

/**
 * An amount that may be below 0, for an opening balance: as `amountString` with zero allowed, and a minus in front for a negative one
 * ("-120", "-120,5"). A minus on nothing or on zero, a plus, and a space after the minus return null, as the app refuses them.
 */
export function signedAmountString(raw: string): string | null {
  const text = raw.trim()
  if (!text.startsWith('-')) return amountString(text, true)
  const rest = text.slice(1)
  const amount = /^\d/.test(rest) ? amountString(rest, true) : null
  return amount === null || amount === '0.00' ? null : `-${amount}`
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

/**
 * An amount in a currency, the way the locale writes it ("€12.00", "RON 12.00"). A currency or a locale that `Intl` cannot use does not throw: the
 * amount is written plainly, behind the code when there is one ("ABCD 12.00").
 */
export function formatMoney(amount: string, currency: string, locale: string): string {
  const value = Number(amount)
  if (!Number.isFinite(value)) return amount
  return currencyFormatter(locale, currency)?.format(value) ?? (currency === '' ? amount : `${currency} ${amount}`)
}

/**
 * An amount written so that it may break after its currency code: the locale ties the two with a no-break space, and a very long amount in a
 * narrow column ("RON 9,999,999,999.99") would then reach past it. For the cells that wrap (`is-wrap`); every other amount keeps the locale's own.
 */
export function breakable(text: string): string {
  return text.replace(/\u00a0/g, ' ')
}

/** `formatMoney` for one currency and locale, for the amounts of an account in another currency than the app's: `moneyIn('RON', 'en-GB')('12.5')`. */
export function moneyIn(currency: string, locale: string): (amount: string) => string {
  return (amount) => formatMoney(amount, currency, locale)
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

/**
 * An amount turned from one currency into another through the main currency, at two typed rates, rounding half up to the cent as the app does.
 * A rate is how many units of that currency one unit of the main currency buys ("5.00" for 1 EUR = 5.00 RON); null is the main currency itself
 * (rate 1). `convertBetween('113.74', '5.00', null)` is what that amount is worth in the main currency, `convertBetween('20.00', null, '5.00')` the
 * other way, and with two rates one exact division and multiplication (not two roundings). Null for a bad amount or rate.
 */
export function convertBetween(amount: string, fromRate: string | null, toRate: string | null): string | null {
  const from = fromRate === null ? { numerator: 1n, denominator: 1n } : parseRate(fromRate)
  const to = toRate === null ? { numerator: 1n, denominator: 1n } : parseRate(toRate)
  const match = /^(\d{1,10})\.(\d{2})$/.exec(amount)
  if (!from || !to || !match) return null
  const minor = BigInt(`${match[1]}${match[2]}`)
  // minor x to / from, as one fraction: (minor x to.numerator x from.denominator) / (to.denominator x from.numerator)
  const numerator = minor * to.numerator * from.denominator
  const denominator = to.denominator * from.numerator
  return fromMinor((numerator * 2n + denominator) / (2n * denominator))
}

/** What an amount in the account's currency is worth in the main one: `amount` divided by the rate, rounded half up to the cent. Null for a bad amount or rate. */
export function invertAmount(amount: string, rate: string): string | null {
  return convertBetween(amount, rate, null)
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

/** An amount such as "-12.50" as whole cents. Null if it is not a two-decimal amount. */
export function toMinor(amount: string): bigint | null {
  const match = /^(-?)(\d+)\.(\d{2})$/.exec(amount)
  if (!match) return null
  const cents = BigInt(`${match[2]}${match[3]}`)
  return match[1] ? -cents : cents
}

/** Whole cents back to a two-decimal amount. */
export function fromMinor(cents: bigint): string {
  const sign = cents < 0n ? '-' : ''
  const text = (cents < 0n ? -cents : cents).toString().padStart(3, '0')
  return `${sign}${text.slice(0, -2)}.${text.slice(-2)}`
}

const percentFormatter = memoByKey(
  (locale: string) => locale,
  (locale: string) => {
    try {
      return new Intl.NumberFormat(locale, { maximumFractionDigits: 1 })
    } catch {
      return null
    }
  },
)

/** 89.1 as "89.1%" in the person's number format. */
export function formatPercent(value: number, locale: string): string {
  return `${percentFormatter(locale)?.format(value) ?? String(value)}%`
}
