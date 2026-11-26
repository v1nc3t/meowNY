import { useT } from '../../../i18n/context'
import { formatMoney, toMinor } from '../../../lib/money'
import Facts from '../../Facts'
import { useWidget } from '../context'
import { loadWorth, useData } from '../data'
import WidgetFrame from '../WidgetFrame'

/** What the accounts hold, what is owed on loans, what friends owe, and the net worth. */
export default function NetWorthWidget() {
  const t = useT()
  const { money } = useWidget()
  const worth = useData('worth', loadWorth)
  const fmt = (amount: string) => formatMoney(amount, money.currency, money.locale)
  return (
    <WidgetFrame to="/app/accounts">
      {worth ? (
        <Facts
          className="dash-facts is-sum"
          items={[
            { label: t('accounts.inAccounts'), value: fmt(worth.inAccounts) },
            toMinor(worth.debts) ? { label: t('loans.title'), value: fmt(worth.debts) } : null,
            toMinor(worth.friends) ? { label: t('accounts.friends'), value: fmt(worth.friends) } : null,
            { label: t('accounts.netWorth'), value: <b>{fmt(worth.netWorth)}</b> },
          ]}
        />
      ) : null}
    </WidgetFrame>
  )
}
