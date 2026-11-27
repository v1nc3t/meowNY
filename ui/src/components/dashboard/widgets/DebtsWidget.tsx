import { useT } from '../../../i18n/context'
import { SPECS } from '../../../lib/dashboardWidgets'
import { formatMoney } from '../../../lib/money'
import { useWidget } from '../context'
import { loadOwed, useData } from '../data'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** What friends owe you and what you owe them, and the friends with the largest balances. */
export default function DebtsWidget() {
  const t = useT()
  const { widget, money } = useWidget()
  const owed = useData('owed', loadOwed)
  const count = widget.settings.count ?? SPECS.debts.settings.count?.start ?? 3
  const fmt = (amount: string) => formatMoney(amount, money.currency, money.locale)
  return (
    <WidgetFrame to="/app/debts">
      {owed ? (
        owed.people.length === 0 ? (
          <Empty text={t('debts.nobody')} />
        ) : (
          <>
            <div className="dash-stats">
              <div className="dash-stat">
                <span className="dash-stat-label">{t('debts.owedToYou')}</span>
                <span className="dash-stat-line">
                  <span className="dash-stat-value">{fmt(owed.owedToYou)}</span>
                </span>
              </div>
              <div className="dash-stat">
                <span className="dash-stat-label">{t('debts.youOwe')}</span>
                <span className="dash-stat-line">
                  <span className="dash-stat-value">{fmt(owed.youOwe)}</span>
                </span>
              </div>
            </div>
            <ul className="dash-legend">
              {owed.people.slice(0, count).map((person) => (
                <li key={person.id}>
                  <span className="legend-name">{person.name}</span>
                  <span className={person.balance.startsWith('-') ? 'legend-amount rec-over' : 'legend-amount rec-paid'}>{fmt(person.balance)}</span>
                </li>
              ))}
            </ul>
          </>
        )
      ) : null}
    </WidgetFrame>
  )
}
