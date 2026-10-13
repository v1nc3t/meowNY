import { en, type MessageKey } from './en'

export type Messages = Record<MessageKey, string>

/** The language the app is shown in right now. The provider sets it; plain functions read it. */
let active = 'en'
let dictionaries: Record<string, Messages> = { en }

export function registerDictionaries(next: Record<string, Messages>): void {
  dictionaries = { ...next, en }
}

export function setActiveLanguage(language: string): void {
  active = language
}

export function activeLanguage(): string {
  return active
}

/** "{name}" placeholders are replaced from `vars`. A missing variable is left as written. */
export function interpolate(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in vars ? String(vars[name]) : whole))
}

/** Text for `key` in `language`, falling back to the base language (e.g. pt-BR to pt), then to English. */
export function translateIn(language: string, key: MessageKey, vars?: Record<string, string | number>): string {
  const base = language.split('-')[0]
  const text = dictionaries[language]?.[key] ?? dictionaries[base]?.[key] ?? en[key]
  return interpolate(text, vars)
}

/** Text in the current language. Safe to call from any code, not only components. */
export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  return translateIn(active, key, vars)
}
