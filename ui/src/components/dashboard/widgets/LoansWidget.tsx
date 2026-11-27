import { Link } from 'react-router-dom'
import { useI18n } from '../../../i18n/context'
import { loanFigures } from '../../../lib/dashboardData'
import { moneyIn, toMinor } from '../../../lib/money'
import { liabilities, totalOwed } from '../../../lib/loansView'
import { monthWords } from '../../../lib/stashView'
import { useWidget } from '../context'
import { loadAccounts, useData } from '../data'
import WidgetFrame, { Empty } from '../WidgetFrame'
import { Stat } from './SummaryWidget'

/**
 * What you owe on loans, what they cost each month and when the last is paid off (the smaller figures go when the widget is narrow), and, when it is
 * tall, each loan with what is owed on it.
 */
export default function LoansWidget() {
  const { t } = useI18n()
  const { money } = useWidget()
  const accounts = useData('accounts', loadAccounts)
  const figures = accounts ? loanFigures(accounts) : null
  const owing = (accounts ? liabilities(accounts) : []).filter((account) => !account.archived && account.liability?.state === 'OWING')
  const payoff = figures?.payoff
  return (
    <WidgetFrame to="/app/debts?tab=debts">
      {accounts && figures ? (
        owing.length === 0 ? (
          <Empty text={t('loans.none')} />
        ) : (
          <>
            <div className="dash-trio">
              <Stat label={t('loans.totalOwed')} value={totalOwed(accounts)} />
              {toMinor(figures.installment) ? <Stat label={t('accounts.installment')} value={figures.installment} /> : null}
              {payoff ? <Stat label={t('loans.payoff')} value="" text={'never' in payoff ? t('loans.never') : monthWords(`${payoff.month}-01`, money.locale)} /> : null}
            </div>
            <ul className="dash-legend dash-tall">
              {owing.map((account) => (
                <li key={account.id}>
                  <span className="legend-name">
                    <Link to={`/app/accounts/${account.id}`}>{account.name}</Link>
                  </span>
                  <span className="legend-amount">{moneyIn(account.currency, money.locale)(account.liability?.owed ?? '0.00')}</span>
                </li>
              ))}
            </ul>
          </>
        )
      ) : null}
    </WidgetFrame>
  )
}
