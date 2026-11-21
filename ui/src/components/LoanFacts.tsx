import type { Account } from '../api/accounts'
import { useT } from '../i18n/context'
import { payoffWords, rateWords } from '../lib/loansView'
import { Card } from './ui/Card'

/**
 * What a credit, a loan or a mortgage owes, costs and when it ends, on the page of the account: what is owed, the rate, the installment, a month of
 * interest and the payoff (a month, or "never" when the installment does not beat the interest), and the state once nothing is owed any more. A line
 * is left out when the account has nothing to say for it. The numbers are the app's (`Account.liability`); `money` writes an amount in the account's
 * currency.
 */
export default function LoanFacts({ account, money, locale }: { account: Account; money: (amount: string) => string; locale: string }) {
  const t = useT()
  const liability = account.liability
  const rows: [string, string][] = [
    [t('accounts.owed'), liability ? money(liability.owed) : ''],
    [t('accounts.rate'), rateWords(account, t)],
    [t('accounts.installment'), account.installment === null ? '' : money(account.installment)],
    [t('loans.interestPerMonth'), liability?.interestPerMonth == null ? '' : money(liability.interestPerMonth)],
    [t('loans.payoff'), payoffWords(liability?.payoff, locale, t)],
    [t('debts.colStatus'), liability && liability.state !== 'OWING' ? t(`loans.state.${liability.state}`) : ''],
  ]
  const shown = rows.filter(([, value]) => value !== '')
  if (shown.length === 0) return null
  return (
    <Card>
      <div className="dash-rec-sum">
        {shown.map(([label, value]) => (
          <span key={label}>
            <span className="dash-stat-label">{label}</span>
            <b>{value}</b>
          </span>
        ))}
      </div>
    </Card>
  )
}
