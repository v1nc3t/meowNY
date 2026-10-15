import { memoByKey } from '../lib/cache'
import { activeLanguage } from '../i18n/translate'

/** Currencies offered in the picker. Any three-letter code is accepted by the backend; this is the list people choose from. */
export const CURRENCIES = [
  ['AED', 'UAE dirham'],
  ['ARS', 'Argentine peso'],
  ['AUD', 'Australian dollar'],
  ['BDT', 'Bangladeshi taka'],
  ['BGN', 'Bulgarian lev'],
  ['BRL', 'Brazilian real'],
  ['CAD', 'Canadian dollar'],
  ['CHF', 'Swiss franc'],
  ['CLP', 'Chilean peso'],
  ['CNY', 'Chinese yuan'],
  ['COP', 'Colombian peso'],
  ['CZK', 'Czech koruna'],
  ['DKK', 'Danish krone'],
  ['EGP', 'Egyptian pound'],
  ['EUR', 'Euro'],
  ['GBP', 'British pound'],
  ['GHS', 'Ghanaian cedi'],
  ['HKD', 'Hong Kong dollar'],
  ['HUF', 'Hungarian forint'],
  ['IDR', 'Indonesian rupiah'],
  ['ILS', 'Israeli shekel'],
  ['INR', 'Indian rupee'],
  ['ISK', 'Icelandic króna'],
  ['JPY', 'Japanese yen'],
  ['KES', 'Kenyan shilling'],
  ['KRW', 'South Korean won'],
  ['MAD', 'Moroccan dirham'],
  ['MXN', 'Mexican peso'],
  ['MYR', 'Malaysian ringgit'],
  ['NGN', 'Nigerian naira'],
  ['NOK', 'Norwegian krone'],
  ['NZD', 'New Zealand dollar'],
  ['PEN', 'Peruvian sol'],
  ['PHP', 'Philippine peso'],
  ['PKR', 'Pakistani rupee'],
  ['PLN', 'Polish złoty'],
  ['QAR', 'Qatari riyal'],
  ['RON', 'Romanian leu'],
  ['RSD', 'Serbian dinar'],
  ['SAR', 'Saudi riyal'],
  ['SEK', 'Swedish krona'],
  ['SGD', 'Singapore dollar'],
  ['THB', 'Thai baht'],
  ['TRY', 'Turkish lira'],
  ['TWD', 'New Taiwan dollar'],
  ['UAH', 'Ukrainian hryvnia'],
  ['USD', 'US dollar'],
  ['VND', 'Vietnamese dong'],
  ['ZAR', 'South African rand'],
] as const

const displayNames = memoByKey(
  (language: string) => language,
  (language: string) => {
    try {
      return new Intl.DisplayNames([language], { type: 'currency' })
    } catch {
      return null
    }
  },
)

/** The currency's name in `language` when the system knows it, else the English name from the list, else ''. */
export function currencyName(code: string, language = activeLanguage()): string {
  try {
    const name = displayNames(language)?.of(code)
    if (name && name !== code) return name
  } catch {
    // Fall back to the English name below.
  }
  return CURRENCIES.find(([known]) => known === code)?.[1] ?? ''
}

/** "EUR  euro", or just the code for one with no known name. */
export function currencyLabel(code: string, language = activeLanguage()): string {
  const name = currencyName(code, language)
  return name ? `${code}  ${name.toLowerCase()}` : code
}

const optionsFor = memoByKey(
  (language: string) => language,
  (language: string) => CURRENCIES.map(([code]) => ({ value: code, label: currencyLabel(code, language) })),
)

/** The picker's options. The list is built once per language and shared; do not change it. */
export function currencyOptions(language = activeLanguage()): { value: string; label: string }[] {
  return optionsFor(language)
}
