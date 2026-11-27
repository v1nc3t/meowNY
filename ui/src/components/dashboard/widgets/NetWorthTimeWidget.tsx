import { useI18n } from '../../../i18n/context'
import { monthsOf } from '../../../lib/dashboardPeriod'
import { SPECS } from '../../../lib/dashboardWidgets'
import { usePeriodRange, useWidget } from '../context'
import { loadNetWorthByMonth, useData } from '../data'
import { rangeLabel } from '../format'
import MonthChart from '../MonthChart'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** What the accounts and debts hold at the end of each month, as a line. */
export default function NetWorthTimeWidget() {
  const { t, language } = useI18n()
  const { month, today } = useWidget()
  const range = usePeriodRange()
  const months = monthsOf(range)
  // months that have not happened yet have no point
  const drawn = months.filter((each) => each <= today.month)
  const points = useData(`${range.from}:${range.to}:${today.month}`, () => loadNetWorthByMonth(drawn), drawn.length > 0)
  return (
    <WidgetFrame to="/app/accounts" aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {drawn.length === 0 ? <Empty text={t('dashboard.emptyPeriod')} /> : null}
      {points && drawn.length > 0 ? <MonthChart months={months} kind="line" label={t(SPECS.networthtime.name)} month={month} keys={[{ id: 'balance', label: t('accounts.netWorth'), amounts: points.map((point) => point.value) }]} /> : null}
    </WidgetFrame>
  )
}
