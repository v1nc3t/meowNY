import { useT } from '../../../i18n/context'
import { SPECS } from '../../../lib/dashboardWidgets'
import { useWidget } from '../context'
import { filterKey, loadNewest, useData } from '../data'
import { useFilter } from '../filter'
import TransactionRows from '../TransactionRows'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** The newest transactions, up to today. */
export default function RecentWidget() {
  const t = useT()
  const { widget, today } = useWidget()
  const { filter, ready } = useFilter()
  const count = widget.settings.count ?? SPECS.recent.settings.count?.start ?? 5
  const rows = useData(`${filterKey(filter)}:${count}:${today.date}`, () => loadNewest(filter, count, today.date), ready)
  return <WidgetFrame to="/app/transactions">{rows ? rows.length === 0 ? <Empty text={t('tx.empty')} /> : <TransactionRows rows={rows} /> : null}</WidgetFrame>
}
