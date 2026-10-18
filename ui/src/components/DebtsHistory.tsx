import { Link } from 'react-router-dom'
import type { History, HistoryLine } from '../api/debts'
import { useT } from '../i18n/context'
import { formatDate, formatMoney } from '../lib/money'

/**
 * The history of debts, newest first, a page at a time: what changed with each friend, and the recurring payments that were paid or are
 * due. A friend's line leads to the friend's statement, a recurring payment's to the recurring screen.
 */
export default function DebtsHistory({
  history,
  currency,
  locale,
  onPage,
}: {
  history: History
  currency: string
  locale: string
  onPage: (offset: number) => void
}) {
  const t = useT()
  const money = (amount: string) => formatMoney(amount, currency, locale)
  const { lines, total, limit, offset } = history

  const effectText = (effect: string) => `${effect.startsWith('-') ? '-' : '+'}${money(effect.replace('-', ''))}`
  const kindText = (line: HistoryLine) => t(`debts.kind.${line.kind}`)

  return (
    <>
      {lines.length === 0 ? <p>{t('debts.noLines')}</p> : null}
      <ul className="card-list">
        {lines.map((line, at) => {
          const recurring = line.recurringId !== null
          // what a friend owes you is green and what you owe is red; a payment you made (late) is money paid, so it has no colour
          const tone = line.effect !== null ? (line.effect.startsWith('-') ? 'debt-owe' : 'debt-owed') : line.kind === 'PAYMENT_DUE' ? 'debt-owe' : ''
          return (
            <li key={`${line.date}-${line.kind}-${line.entryId ?? line.transactionId ?? line.recurringId ?? ''}-${offset + at}`} className="card-item">
              <div className="card-body">
                <p className="card-name">
                  {recurring ? (
                    <Link to="/app/recurring" state={{ focus: { kind: 'recurring', id: line.recurringId, date: line.date } }}>
                      {line.what}
                    </Link>
                  ) : line.personId ? (
                    <Link to={`/app/debts/${line.personId}`}>{line.friend}</Link>
                  ) : (
                    line.friend
                  )}
                  <span className="muted">{` · ${kindText(line)}`}</span>
                  {!recurring && line.what ? <span className="muted">{` · ${line.what}`}</span> : null}
                </p>
                <p className="card-meta">
                  <span className={`card-amount ${tone}`}>
                    {line.effect !== null
                      ? effectText(line.effect)
                      : line.kind === 'PAYMENT_DUE' && Number(line.amount) === 0
                        ? t('rec.varies')
                        : money(line.amount)}
                  </span>
                  {` · ${formatDate(line.date, locale)}`}
                  {line.dueDate && line.dueDate !== line.date ? ` · ${t('debts.dueOn', { date: formatDate(line.dueDate, locale) })}` : ''}
                  {line.balance !== null ? ` · ${t('debts.balance')} ${money(line.balance)}` : ''}
                </p>
              </div>
            </li>
          )
        })}
      </ul>
      {total > limit ? (
        <div className="tx-actions">
          <button type="button" disabled={offset + limit >= total} onClick={() => onPage(offset + limit)}>
            {t('debts.older')}
          </button>
          <button type="button" disabled={offset === 0} onClick={() => onPage(Math.max(0, offset - limit))}>
            {t('debts.newer')}
          </button>
          <span className="muted">{t('debts.historyPage', { from: offset + 1, to: Math.min(offset + limit, total), total })}</span>
        </div>
      ) : null}
    </>
  )
}
