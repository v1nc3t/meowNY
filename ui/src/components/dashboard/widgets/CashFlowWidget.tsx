import { useI18n } from '../../../i18n/context'
import { monthsOf } from '../../../lib/dashboardPeriod'
import { SPECS } from '../../../lib/dashboardWidgets'
import { usePeriodRange, useWidget } from '../context'
import { filterKey, loadPoints, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import MonthChart from '../MonthChart'
import WidgetFrame from '../WidgetFrame'

/** Money in and out by month, a pair of bars for each month. */
export default function CashFlowWidget() {
  const { t, language } = useI18n()
  const { month, today } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const points = useData(`${range.from}:${range.to}:${filterKey(filter)}`, () => loadPoints(range, filter), ready)
  // months that have not happened yet have no bars
  const drawn = (points ?? []).filter((point) => point.month <= today.month)
  return (
    <WidgetFrame aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {points ? (
        <MonthChart
          months={monthsOf(range)}
          kind="bars"
          label={t(SPECS.cashflow.name)}
          month={month}
          keys={[
            { id: 'income', label: t('common.income'), amounts: drawn.map((point) => point.income) },
            { id: 'expense', label: t('common.expense'), amounts: drawn.map((point) => point.expense) },
          ]}
        />
      ) : null}
    </WidgetFrame>
  )
}
