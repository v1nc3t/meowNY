import { Link } from 'react-router-dom'
import type { History, HistoryLine } from '../api/debts'
import { useGoToTransaction } from '../hooks/useFocusRequest'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { noteOf } from '../lib/balance'
import { formatDate, formatMoney } from '../lib/money'
import Facts from './Facts'
import { DayGroups, IconAction, NoteSection, PanelActions, Row, Table, TapRow, type PanelAction } from './RowParts'
import { Button } from './ui/button'

/**
 * The debts of all friends, newest first, a page at a time: what changed with each friend. A friend's name leads to the friend's history.
 * A line with a note opens to it, and one with a transaction (a bill, or a payment that was for one) has a way to go to it.
 */
export default function DebtsHistory({
  history,
  currency,
  locale,
  today,
  onPage,
}: {
  history: History
  currency: string
  locale: string
  /** Today's date, for the year of a day's heading on the phone. */
  today: string
  onPage: (offset: number) => void
}) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const rows = useOpenRow()
  const goToTransaction = useGoToTransaction()
  const money = (amount: string) => formatMoney(amount, currency, locale)
  const { lines, total, limit, offset } = history

  const effectText = (effect: string) => `${effect.startsWith('-') ? '-' : '+'}${money(effect.replace('-', ''))}`
  const kindText = (line: HistoryLine) => t(`debts.kind.${line.kind}`)
  const keyOf = (line: HistoryLine, at: number) => `${line.date}-${line.kind}-${line.entryId ?? line.transactionId ?? ''}-${offset + at}`

  /** The friend, leading to their page; a recurring payment has no friend, and is named by what it is. */
  const subject = (line: HistoryLine) => (line.personId ? <Link to={`/app/debts/${line.personId}`}>{line.friend}</Link> : (line.friend ?? line.what))
  /** The same, as text. */
  const subjectText = (line: HistoryLine) => line.friend ?? line.what
  /** The name of a bill, in the panel of its row: not for a recurring payment (that is its name above) and not for an entry (its note is in the panel). */
  const billOf = (line: HistoryLine) => (line.kind === 'BILL' ? line.what : '')

  /** The change: what a friend owes you more is green and what you owe is red. */
  function amount(line: HistoryLine) {
    const effect = line.effect ?? line.amount
    return <span className={`cell-amount ${effect.startsWith('-') ? 'debt-owe' : 'debt-owed'}`}>{effectText(effect)}</span>
  }

  // A desktop row: who, what kind of change, the day, the change and the balance after it, then the button that goes to its
  // transaction and the chevron of a bill's name or a note, which opens under the row.
  function tableRow(line: HistoryLine, at: number) {
    const note = noteOf(line)
    const bill = billOf(line)
    const key = keyOf(line, at)
    return (
      <Row
        key={key}
        rows={rows}
        id={key}
        actions={line.transactionId ? <IconAction icon="transactions" label={t('debts.goToTransaction')} data-row-focus onClick={() => goToTransaction(line)} /> : null}
        panel={
          note || bill ? (
            <>
              <Facts items={[{ label: t('code.what'), value: bill }]} className="acct-facts" />
              {note ? <NoteSection note={note} /> : null}
            </>
          ) : null
        }
      >
        <div className="cell-name">
          <span className="card-name">{subject(line)}</span>
        </div>
        <span className="cell-sub" title={kindText(line)}>
          {kindText(line)}
        </span>
        <span className="cell-sub-end">{formatDate(line.date, locale)}</span>
        {amount(line)}
        <span className="cell-bal">{line.balance !== null ? money(line.balance) : ''}</span>
      </Row>
    )
  }

  // A phone row: who and the change, what kind of change it was under them. It opens to the day, the balance after it, a bill's name, its note, and
  // buttons for its transaction and for the friend (a recurring payment has none).
  function tapRow(line: HistoryLine) {
    const key = keyOf(line, lines.indexOf(line))
    const note = noteOf(line)
    const actions: (PanelAction | false)[] = [
      !!line.transactionId && { label: t('debts.goToTransaction'), icon: 'transactions', onClick: () => goToTransaction(line) },
      !!line.personId && { label: line.friend ?? '', icon: 'account', to: `/app/debts/${line.personId}` },
    ]
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={subjectText(line)}
        amount={amount(line)}
        meta={kindText(line)}
        facts={[
          { label: t('sync.field.date'), value: formatDate(line.date, locale) },
          { label: t('debts.balance'), value: line.balance !== null ? money(line.balance) : null },
          { label: t('code.what'), value: billOf(line) },
        ]}
      >
        {note ? <NoteSection note={note} /> : null}
        <PanelActions actions={actions} />
      </TapRow>
    )
  }

  return (
    <>
      {lines.length === 0 ? <p className="hint">{t('debts.noLines')}</p> : null}
      {lines.length > 0 && phone ? (
        <DayGroups rows={lines} dateOf={(line) => line.date} locale={locale} today={today}>
          {tapRow}
        </DayGroups>
      ) : null}
      {lines.length > 0 && !phone ? (
        <Table
          className="history-table"
          tidy
          heads={[t('common.name'), t('debts.kind'), t('sync.field.date'), { label: t('debts.change'), className: 'is-end' }, { label: t('debts.balance'), className: 'is-end' }, '']}
          opens
        >
          {lines.map(tableRow)}
        </Table>
      ) : null}
      {total > limit ? (
        <div className="tx-actions debts-pager">
          <Button variant="outline" size="sm" disabled={offset + limit >= total} onClick={() => onPage(offset + limit)}>
            {t('debts.older')}
          </Button>
          <Button variant="outline" size="sm" disabled={offset === 0} onClick={() => onPage(Math.max(0, offset - limit))}>
            {t('debts.newer')}
          </Button>
          <span className="muted">{t('debts.historyPage', { from: offset + 1, to: Math.min(offset + limit, total), total })}</span>
        </div>
      ) : null}
    </>
  )
}
