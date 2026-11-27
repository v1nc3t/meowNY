import { useI18n } from '../../../i18n/context'
import { formatMoney } from '../../../lib/money'
import { useWidget } from '../context'
import { loadAccountCard, useData } from '../data'
import { useFilter } from '../filter'
import Spark from '../Spark'
import WidgetFrame from '../WidgetFrame'

/** One account: its balance, what moved it this month (green up, red down) and its balance month by month. */
export default function AccountWidget() {
  const { t } = useI18n()
  const { money, today } = useWidget()
  const { filter, ready } = useFilter()
  const id = filter.accountIds?.[0]
  const loaded = useData(`${id ?? ''}:${today.month}`, () => loadAccountCard(id, today.month, today.month).then((card) => ({ card })), ready)
  const card = loaded?.card
  if (!loaded) return <WidgetFrame />
  if (!card) return <WidgetFrame />
  const { account } = card
  const fmt = (amount: string) => formatMoney(amount, account.currency, money.locale)
  const change = account.changeThisMonth
  const tone = change.startsWith('-') ? ' is-bad' : change === '0.00' ? '' : ' is-good'
  return (
    <WidgetFrame to={`/app/accounts/${account.id}`} name={account.name}>
      <div className="dash-stat">
        <span className="dash-stat-label is-optional">{t('accounts.balance')}</span>
        <span className="dash-stat-line">
          <span className="dash-stat-value">{fmt(account.balance)}</span>
          <span className={`dash-delta${tone}`} title={t('accounts.thisMonth')}>
            {change.startsWith('-') ? '▼ ' : change === '0.00' ? '' : '▲ '}
            {fmt(change.replace('-', ''))}
          </span>
        </span>
      </div>
      <Spark values={card.balances} />
    </WidgetFrame>
  )
}
