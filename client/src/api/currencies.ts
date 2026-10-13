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

export function currencyName(code: string): string {
  return CURRENCIES.find(([known]) => known === code)?.[1] ?? ''
}

/** "EUR  euro", or just the code for one that is not in the list. */
export function currencyLabel(code: string): string {
  const name = currencyName(code)
  return name ? `${code}  ${name.toLowerCase()}` : code
}

export const currencyOptions = CURRENCIES.map(([code]) => ({ value: code, label: currencyLabel(code) }))
