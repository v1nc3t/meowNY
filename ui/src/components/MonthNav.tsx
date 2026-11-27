import { useT } from '../i18n/context'
import PeriodNav from './PeriodNav'

/** "‹ nov 2026 ›": the previous and next month around a choice of month and year; "current" only while another month is shown. */
export default function MonthNav({ month, thisMonth, onChange }: { month: string; thisMonth: string; onChange: (month: string) => void }) {
  const t = useT()
  return <PeriodNav keys kind="month" value={month} current={thisMonth} onChange={onChange} prevLabel={t('budgets.prevMonth')} nextLabel={t('budgets.nextMonth')} />
}
