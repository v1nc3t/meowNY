import { useI18n } from '../../../i18n/context'
import { savingsRateText } from '../../../pages/dashboardMath'
import { usePeriodRange, useWidget } from '../context'
import { filterKey, loadPoints, totalsOf, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import Bar from '../Bar'
import WidgetFrame from '../WidgetFrame'

/** The savings rate of the period as a large number and a bar (empty and red when more was spent than earned). */
export default function SavingsRateWidget() {
  const { language } = useI18n()
  const { money } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const totals = useData(`${range.from}:${range.to}:${filterKey(filter)}`, () => loadPoints(range, filter).then(totalsOf), ready)
  const rate = totals?.savingsRate ?? null
  return (
    <WidgetFrame aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {totals ? (
        <>
          <span className={rate !== null && rate < 0 ? 'dash-stat-value is-big is-bad' : 'dash-stat-value is-big'}>{savingsRateText(rate, money.locale)}</span>
          <Bar percent={rate ?? 0} />
        </>
      ) : null}
    </WidgetFrame>
  )
}
