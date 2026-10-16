import type { MessageKey } from '../i18n/en'
import { pickLanguage } from '../i18n/languages'
import { t } from '../i18n/translate'

// Input rules shared by the first-run screen and settings. The Rust side enforces the same
// rules again; these exist to give an answer before a round trip.

export const MIN_PASSPHRASE_CHARS = 8

/** A library smaller than this (100 MB) goes into a backup unless the person says no; a bigger one only if they say yes. */
export const DOCUMENTS_BY_DEFAULT_UNDER = 100 * 1024 * 1024

/** How many scheduled backups the app keeps in the folder (`KEEP` in `schedule.rs`). */
export const BACKUPS_KEPT = 10

/** Whether the files of the documents are put in a backup when the person has not chosen. */
export function includeDocumentsByDefault(heldBytes: number): boolean {
  return heldBytes < DOCUMENTS_BY_DEFAULT_UNDER
}

/** The lines that follow "backup saved": how many documents it holds, and how many were left out for having no file here. */
export function backupDoneLines(done: { documents: number; missing: number }): { key: MessageKey; count: number }[] {
  const lines: { key: MessageKey; count: number }[] = [
    { key: 'settings.backupHolds', count: done.documents },
    { key: 'settings.backupMissing', count: done.missing },
  ]
  return lines.filter((line) => line.count > 0)
}

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

/** Like `passphraseProblem`, but a phone always needs a passphrase: there is no keychain to rely on. */
export function phonePassphraseProblem(passphrase: string, confirm: string): string {
  if (passphrase === '') return t('phone.passphraseRequired')
  return passphraseProblem(passphrase, confirm)
}

/** Like `passphraseProblem`, but a backup always needs a passphrase. */
export function backupPassphraseProblem(passphrase: string, confirm: string): string {
  if ([...passphrase].length < MIN_PASSPHRASE_CHARS) return t('rule.backupPassphraseShort', { count: MIN_PASSPHRASE_CHARS })
  return passphraseProblem(passphrase, confirm)
}

export const LOCK_AFTER_CHOICES = [
  { seconds: 60, labelKey: 'lock.1m' },
  { seconds: 300, labelKey: 'lock.5m' },
  { seconds: 900, labelKey: 'lock.15m' },
  { seconds: 3600, labelKey: 'lock.1h' },
  { seconds: 0, labelKey: 'lock.never' },
] as const

/** The choices of "lock after" a device offers. A phone does not offer "never": it is the one that gets lost. */
export function lockChoices(phone: boolean): readonly (typeof LOCK_AFTER_CHOICES)[number][] {
  return phone ? LOCK_AFTER_CHOICES.filter((choice) => choice.seconds > 0) : LOCK_AFTER_CHOICES
}

/** The nearest offered choice, so a stored value that is not in the list still shows up. */
export function nearestLockChoice(seconds: number, choices: readonly { seconds: number }[] = LOCK_AFTER_CHOICES): number {
  return choices.reduce((best, choice) => (Math.abs(choice.seconds - seconds) < Math.abs(best.seconds - seconds) ? choice : best)).seconds
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
