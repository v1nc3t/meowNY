import { Link } from 'react-router-dom'
import { useWidget } from '../context'
import { loadAccounts, useData } from '../data'
import { formatMoney } from '../../../lib/money'
import WidgetFrame from '../WidgetFrame'

/** Every account (not the archived ones) with its balance, in the account's own currency; the first few when a count is set. */
export default function AccountsWidget() {
  const { widget, money } = useWidget()
  const accounts = useData('accounts', loadAccounts)
  const shown = (accounts ?? []).filter((account) => !account.archived).slice(0, widget.settings.count ?? undefined)
  return (
    <WidgetFrame to="/app/accounts">
      {accounts ? (
        <ul className="dash-legend">
          {shown.map((account) => (
            <li key={account.id}>
              <span className="legend-name">
                <Link to={`/app/accounts/${account.id}`}>{account.name}</Link>
              </span>
              <span className={account.balance.startsWith('-') ? 'legend-amount rec-over' : 'legend-amount'}>{formatMoney(account.balance, account.currency, money.locale)}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </WidgetFrame>
  )
}
