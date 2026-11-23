import { withYear } from '../api/months'
import { useT } from '../i18n/context'
import PeriodNav from './PeriodNav'

/** "‹ 2026 ›": the previous and next year around a choice of year; "current" only while another year is shown. Keeps the month, so March 2026 becomes March 2025. */
export default function YearNav({ month, thisMonth, onChange }: { month: string; thisMonth: string; onChange: (month: string) => void }) {
  const t = useT()
  const year = Number(month.slice(0, 4))
  return (
    <PeriodNav
      kind="year"
      value={String(year)}
      current={thisMonth.slice(0, 4)}
      onChange={(next) => onChange(withYear(month, Number(next)))}
      onCurrent={() => onChange(thisMonth)}
      prevLabel={t('dashboard.prevYear')}
      nextLabel={t('dashboard.nextYear')}
    />
  )
}
