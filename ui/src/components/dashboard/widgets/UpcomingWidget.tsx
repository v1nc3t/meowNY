import { Link } from 'react-router-dom'
import type { Recurring } from '../../../api/recurring'
import { useT } from '../../../i18n/context'
import { SPECS } from '../../../lib/dashboardWidgets'
import { formatDate, formatMoney } from '../../../lib/money'
import { useWidget } from '../context'
import { loadDues, useData } from '../data'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** How many days ahead "upcoming" looks. */
const AHEAD_DAYS = 30

/** The recurring items that are overdue (their day in red) and then those due in the next days, each leading to the item. */
export default function UpcomingWidget() {
  const t = useT()
  const { widget, money, today } = useWidget()
  const dues = useData('dues', () => loadDues(AHEAD_DAYS))
  const count = widget.settings.count ?? SPECS.upcoming.settings.count?.start ?? 10
  const rows = dues ? [...dues.overdue.map((item) => ({ item, late: true })), ...dues.upcoming.map((item) => ({ item, late: false }))].slice(0, count) : []
  const amount = (item: Recurring) => (item.amount === null ? t('rec.varies') : formatMoney(item.amount, money.currency, money.locale))
  return (
    <WidgetFrame to="/app/recurring">
      {dues ? (
        rows.length === 0 ? (
          <Empty text={t('dashboard.recSoonEmpty', { days: AHEAD_DAYS })} />
        ) : (
          <ul className="dash-legend">
            {rows.map(({ item, late }) => (
              <li key={item.id}>
                <span className="legend-name">
                  <Link to="/app/recurring" state={{ focus: { kind: 'recurring', id: item.id, date: item.nextDueDate } }}>
                    {item.name}
                  </Link>
                </span>
                <span className={late ? 'legend-pct rec-over' : 'legend-pct'}>{item.nextDueDate === today.date ? t('rec.dueToday') : formatDate(item.nextDueDate, money.locale)}</span>
                <span className="legend-amount">{amount(item)}</span>
              </li>
            ))}
          </ul>
        )
      ) : null}
    </WidgetFrame>
  )
}
