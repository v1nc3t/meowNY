import { de } from './de'
import { en } from './en'
import { es } from './es'
import { fr } from './fr'
import { it } from './it'
import { pt } from './pt'
import { ro } from './ro'
import { registerDictionaries, type Messages } from './translate'

/** Languages the app can be shown in. The name is written in the language itself, so people can find theirs. */
export const LANGUAGES: readonly { code: string; name: string; messages: Messages }[] = [
  { code: 'en', name: 'English', messages: en },
  { code: 'de', name: 'Deutsch', messages: de },
  { code: 'es', name: 'Español', messages: es },
  { code: 'fr', name: 'Français', messages: fr },
  { code: 'it', name: 'Italiano', messages: it },
  { code: 'pt', name: 'Português', messages: pt },
  { code: 'ro', name: 'Română', messages: ro },
]

registerDictionaries(Object.fromEntries(LANGUAGES.map((language) => [language.code, language.messages])))

export const languageOptions = LANGUAGES.map((language) => ({ value: language.code, label: language.name }))

/** The supported language that best matches a list of preferences such as the system's. English if none do. */
export function pickLanguage(preferred: readonly string[]): string {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split('-')[0]
    if (LANGUAGES.some((language) => language.code === base)) return base
  }
  return 'en'
}
