import { activeLanguage, t } from '../i18n/translate'

/** Every fixed piece of text on a PDF report, in the language the app is shown in. */
export type ReportLabels = {
  title: string
  period: string
  income: string
  expenses: string
  savings: string
  savingsRate: string
  byMonth: string
  spending: string
  other: string
  budgetVsActual: string
  noBudget: string
  transactions: string
  date: string
  category: string
  name: string
  amount: string
  page: string
  noData: string
}

const capital = (text: string) => text.charAt(0).toLocaleUpperCase(activeLanguage()) + text.slice(1)

/** "October 2026", or "October 2026 – December 2026" for a range, in the person's number and date format. */
export function periodLabel(fromMonth: string, toMonth: string, locale: string): string {
  const name = (month: string) => {
    const [year, number] = month.split('-').map(Number)
    try {
      return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(
        new Date(Date.UTC(year, number - 1, 1)),
      )
    } catch {
      return month
    }
  }
  return fromMonth === toMonth ? name(fromMonth) : `${name(fromMonth)} – ${name(toMonth)}`
}

/** The labels for the current language. The app writes them to the PDF as they are. */
export function reportLabels(period: string): ReportLabels {
  return {
    title: t('report.title'),
    period,
    income: capital(t('common.income')),
    expenses: capital(t('budgets.expenses')),
    savings: capital(t('dashboard.savings')),
    savingsRate: capital(t('dashboard.savingsRate')),
    byMonth: capital(t('report.byMonth')),
    spending: capital(t('report.spending')),
    other: capital(t('dashboard.other')),
    budgetVsActual: capital(t('report.budgetVsActual')),
    noBudget: t('report.noBudget'),
    transactions: capital(t('tx.title')),
    date: capital(t('report.date')),
    category: capital(t('common.category')),
    name: capital(t('common.name')),
    amount: capital(t('tx.amount')),
    page: capital(t('report.page')),
    noData: capital(t('report.noData')),
  }
}
