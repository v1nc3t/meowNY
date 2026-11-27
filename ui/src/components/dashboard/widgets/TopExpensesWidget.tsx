import { useI18n } from '../../../i18n/context'
import { SPECS } from '../../../lib/dashboardWidgets'
import { usePeriodRange, useWidget } from '../context'
import { filterKey, loadBiggest, useData } from '../data'
import { useFilter } from '../filter'
import { rangeLabel } from '../format'
import TransactionRows from '../TransactionRows'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** The expenses that cost the most over the period. */
export default function TopExpensesWidget() {
  const { t, language } = useI18n()
  const { widget } = useWidget()
  const range = usePeriodRange()
  const { filter, ready } = useFilter()
  const count = widget.settings.count ?? SPECS.topexpenses.settings.count?.start ?? 5
  const rows = useData(`${range.from}:${range.to}:${filterKey(filter)}:${count}`, () => loadBiggest(range, filter, count), ready)
  return (
    <WidgetFrame to="/app/transactions" aside={<span className="dash-aside">{rangeLabel(range, language)}</span>}>
      {rows ? rows.length === 0 ? <Empty text={t('dashboard.emptyPeriod')} /> : <TransactionRows rows={rows} /> : null}
    </WidgetFrame>
  )
}
