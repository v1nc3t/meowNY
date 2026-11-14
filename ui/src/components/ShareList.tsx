import type { Share } from '../api/accounts'
import { useT } from '../i18n/context'
import { barWidth } from '../lib/accountsView'
import { Card, CardAction, CardHeader, CardTitle } from './ui/Card'

/**
 * What one account spent, or took in, by category over a period: the total in the corner, then a row for each category with its amount
 * and a thin bar (the largest fills it; the bar is only a picture, the amount says it all). An empty list says so.
 */
export default function ShareList({
  title,
  shares,
  total,
  tone,
  money,
}: {
  title: string
  /** Largest first. */
  shares: readonly Share[]
  total: string
  tone: 'expense' | 'income'
  money: (amount: string) => string
}) {
  const t = useT()
  const largest = shares.reduce((most, share) => (Number(share.amount) > Number(most) ? share.amount : most), '0.00')
  return (
    <Card className={`acct-share is-${tone}`}>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardAction>
          <b className="acct-share-total">{money(total)}</b>
        </CardAction>
      </CardHeader>
      {shares.length === 0 ? (
        <p className="hint">{t('accounts.nothingThisMonth')}</p>
      ) : (
        <ul className="acct-share-list">
          {shares.map((share) => (
            <li key={share.categoryId}>
              <span className="acct-share-text">
                <span className="acct-share-name" title={share.name}>
                  {share.name}
                </span>
                <span className="acct-share-amount">{money(share.amount)}</span>
              </span>
              <span className="bar-row" aria-hidden="true">
                <span className="bar">
                  <span className="bar-fill" style={{ width: `${barWidth(share.amount, largest)}%` }} />
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  )
}
