/** A size for people: bytes up to 1 KB, then KB, then MB with one decimal, written the way the locale writes numbers. */
export function formatSize(bytes: number, locale: string): string {
  const one = (value: number) => {
    try {
      return new Intl.NumberFormat(locale, { maximumFractionDigits: 1, minimumFractionDigits: 0 }).format(value)
    } catch {
      return String(Math.round(value * 10) / 10)
    }
  }
  if (!Number.isFinite(bytes) || bytes < 0) return ''
  if (bytes < 1024) return `${one(bytes)} B`
  if (bytes < 1024 * 1024) return `${one(bytes / 1024)} KB`
  return `${one(bytes / (1024 * 1024))} MB`
}
