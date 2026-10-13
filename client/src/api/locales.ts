import { activeLanguage } from '../i18n/translate'

/** Common number and date formats, offered after the ones the system itself prefers. */
const COMMON_LOCALES = [
  'en-US', 'en-GB', 'en-IE', 'en-AU', 'en-CA', 'en-NZ', 'en-IN', 'en-ZA',
  'fr-FR', 'fr-CA', 'fr-BE', 'fr-CH', 'de-DE', 'de-AT', 'de-CH',
  'es-ES', 'es-MX', 'es-AR', 'es-CO', 'es-CL', 'pt-PT', 'pt-BR', 'it-IT',
  'nl-NL', 'nl-BE', 'sv-SE', 'da-DK', 'nb-NO', 'fi-FI', 'is-IS',
  'pl-PL', 'cs-CZ', 'sk-SK', 'hu-HU', 'ro-RO', 'bg-BG', 'hr-HR', 'sl-SI', 'sr-RS',
  'lt-LT', 'lv-LV', 'et-EE', 'el-GR', 'tr-TR', 'ru-RU', 'uk-UA',
  'he-IL', 'ar-SA', 'ar-EG', 'ar-AE', 'fa-IR', 'hi-IN', 'bn-BD',
  'th-TH', 'vi-VN', 'id-ID', 'ms-MY', 'zh-CN', 'zh-TW', 'zh-HK', 'ja-JP', 'ko-KR',
] as const

/** Used when the system cannot list its time zones. */
const COMMON_TIME_ZONES = [
  'UTC', 'Europe/London', 'Europe/Dublin', 'Europe/Lisbon', 'Europe/Paris', 'Europe/Berlin', 'Europe/Madrid',
  'Europe/Rome', 'Europe/Amsterdam', 'Europe/Stockholm', 'Europe/Warsaw', 'Europe/Athens', 'Europe/Helsinki',
  'Europe/Istanbul', 'Europe/Moscow', 'Africa/Cairo', 'Africa/Johannesburg', 'Africa/Lagos', 'Asia/Dubai',
  'Asia/Kolkata', 'Asia/Bangkok', 'Asia/Singapore', 'Asia/Hong_Kong', 'Asia/Shanghai', 'Asia/Tokyo', 'Asia/Seoul',
  'Australia/Sydney', 'Australia/Perth', 'Pacific/Auckland', 'America/St_Johns', 'America/Halifax',
  'America/New_York', 'America/Chicago', 'America/Denver', 'America/Phoenix', 'America/Los_Angeles',
  'America/Anchorage', 'Pacific/Honolulu', 'America/Mexico_City', 'America/Bogota', 'America/Lima',
  'America/Sao_Paulo', 'America/Argentina/Buenos_Aires', 'America/Santiago',
] as const

/**
 * Locales to offer, most relevant first: the one in use, then what the system prefers
 * (the operating system's language list), then the common ones. Duplicates removed, and
 * anything the engine cannot format is dropped.
 */
export function localeChoices(
  current: string,
  preferred: readonly string[],
  isSupported: (tag: string) => boolean,
): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const tag of [current, ...preferred, ...COMMON_LOCALES]) {
    if (!tag || seen.has(tag)) continue
    seen.add(tag)
    if (tag === current || isSupported(tag)) out.push(tag)
  }
  return out
}

/** "en-IE  English (Ireland)", or just the tag when the system has no name for it. */
export function localeLabel(tag: string, language = activeLanguage()): string {
  try {
    const name = new Intl.DisplayNames([language], { type: 'language', languageDisplay: 'standard' }).of(tag)
    return name && name !== tag ? `${tag}  ${name.toLowerCase()}` : tag
  } catch {
    return tag
  }
}

/** A sample of how numbers and dates will look, so the choice is visible before saving. */
export function formatExample(locale: string, currency: string, date = new Date(Date.UTC(2026, 9, 13))): string {
  try {
    const money = new Intl.NumberFormat(locale, { style: 'currency', currency }).format(1234.56)
    const day = new Intl.DateTimeFormat(locale, { day: '2-digit', month: '2-digit', year: 'numeric', timeZone: 'UTC' }).format(date)
    // Non-breaking spaces, so the spacing around the bar survives HTML whitespace collapsing.
    return `${money}\u00a0\u00a0|\u00a0\u00a0${day}`
  } catch {
    return ''
  }
}

/**
 * Time zones to offer: the one in use and the system's own first, UTC, then the rest alphabetically.
 * `all` is what the system can list, or null when it cannot (a short common list is used then).
 */
export function timeZoneChoices(all: readonly string[] | null, system: string, current: string): string[] {
  const rest = [...new Set([...(all ?? COMMON_TIME_ZONES), 'UTC'])].sort()
  const first = [current, system].filter((zone, index, list) => zone && list.indexOf(zone) === index)
  return [...first, ...rest.filter((zone) => !first.includes(zone))]
}

/** Format choices as dropdown options: the system's preferred formats first, then common ones. */
export function localeOptions(current: string, language = activeLanguage()): { value: string; label: string }[] {
  const supported = (tag: string) => Intl.NumberFormat.supportedLocalesOf(tag).length > 0
  return localeChoices(current, typeof navigator === 'undefined' ? [] : navigator.languages ?? [], supported).map(
    (tag) => ({ value: tag, label: localeLabel(tag, language) }),
  )
}

/** Time zone choices as dropdown options: every zone the system knows, the current and the system's own first. */
export function zoneOptions(current: string): { value: string; label: string }[] {
  const all = typeof Intl.supportedValuesOf === 'function' ? Intl.supportedValuesOf('timeZone') : null
  const system = Intl.DateTimeFormat().resolvedOptions().timeZone
  return timeZoneChoices(all, system, current).map((zone) => ({ value: zone, label: zone }))
}
