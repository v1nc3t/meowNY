// Input rules shared by the first-run screen and settings. The Rust side enforces the same
// rules again; these exist to give an answer before a round trip.

export const MIN_PASSPHRASE_CHARS = 8

export type Settings = { currency: string; locale: string; timezone: string }

export function normalizeCurrency(input: string): string {
  return input.trim().toUpperCase()
}

export function isCurrency(input: string): boolean {
  return /^[A-Z]{3}$/.test(normalizeCurrency(input))
}

/** Returns a message when the passphrase pair is not acceptable, or '' when it is fine. An empty passphrase means "keychain only". */
export function passphraseProblem(passphrase: string, confirm: string): string {
  if (passphrase === '') return ''
  if ([...passphrase].length < MIN_PASSPHRASE_CHARS) {
    return `use at least ${MIN_PASSPHRASE_CHARS} characters, or leave it empty.`
  }
  if (passphrase !== confirm) return 'the two passphrases do not match.'
  return ''
}

export const LOCK_AFTER_CHOICES = [
  { seconds: 60, label: '1 minute' },
  { seconds: 300, label: '5 minutes' },
  { seconds: 900, label: '15 minutes' },
  { seconds: 3600, label: '1 hour' },
  { seconds: 0, label: 'never' },
] as const

/** The nearest offered choice, so a stored value that is not in the list still shows up. */
export function nearestLockChoice(seconds: number): number {
  return LOCK_AFTER_CHOICES.reduce((best, choice) =>
    Math.abs(choice.seconds - seconds) < Math.abs(best.seconds - seconds) ? choice : best,
  ).seconds
}

export function defaultSettings(): Settings {
  let locale = 'en'
  let timezone = 'UTC'
  try {
    locale = navigator.language || locale
    timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || timezone
  } catch {
    // Keep the defaults.
  }
  return { currency: '', locale, timezone }
}
