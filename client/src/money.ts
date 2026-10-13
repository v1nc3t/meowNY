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

export function formatMoney(amount: string, currency: string, locale: string): string {
  const value = Number(amount)
  if (!Number.isFinite(value)) return amount
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(value)
  } catch {
    return amount
  }
}
