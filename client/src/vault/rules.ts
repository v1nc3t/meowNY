import { pickLanguage } from '../i18n/languages'
import { t } from '../i18n/translate'

// Input rules shared by the first-run screen and settings. The Rust side enforces the same
// rules again; these exist to give an answer before a round trip.

export const MIN_PASSPHRASE_CHARS = 8

export type Settings = { currency: string; locale: string; timezone: string; language: string }

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
    return t('rule.passphraseShort', { count: MIN_PASSPHRASE_CHARS })
  }
  if (passphrase !== confirm) return t('rule.passphraseMismatch')
  return ''
}

export const LOCK_AFTER_CHOICES = [
  { seconds: 60, labelKey: 'lock.1m' },
  { seconds: 300, labelKey: 'lock.5m' },
  { seconds: 900, labelKey: 'lock.15m' },
  { seconds: 3600, labelKey: 'lock.1h' },
  { seconds: 0, labelKey: 'lock.never' },
] as const

/** The nearest offered choice, so a stored value that is not in the list still shows up. */
export function nearestLockChoice(seconds: number): number {
  return LOCK_AFTER_CHOICES.reduce((best, choice) =>
    Math.abs(choice.seconds - seconds) < Math.abs(best.seconds - seconds) ? choice : best,
  ).seconds
}

/** What the system suggests: its format, its time zone and (if the app has it) its language. */
export function defaultSettings(): Settings {
  let locale = 'en'
  let timezone = 'UTC'
  try {
    locale = navigator.language || locale
    timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || timezone
  } catch {
    // Keep the defaults.
  }
  return { currency: '', locale, timezone, language: pickLanguage(typeof navigator === 'undefined' ? [] : navigator.languages ?? [locale]) }
}
