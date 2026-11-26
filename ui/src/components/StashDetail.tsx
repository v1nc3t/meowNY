import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { Account } from '../api/accounts'
import { stashes as stashesApi, type Detail, type Entry } from '../api/stashes'
import type { Today } from '../api/today'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useProfile } from '../hooks/profile'
import { useI18n } from '../i18n/context'
import { accountById, accountLabel, accountName } from '../lib/accountsView'
import { formatDate } from '../lib/money'
import { overBooked, stashFacts, stashMoney } from '../lib/stashView'
import BalanceChart from './BalanceChart'
import Facts from './Facts'
import { DayGroups, IconAction, PanelActions, Row, Table, TapRow, type PanelAction } from './RowParts'
import { StashBar } from './StashParts'
import YearNav from './YearNav'
import { Card, CardAction, CardHeader } from './ui/card'
import { cn } from './ui/cn'

/** What a stash held at the end of each month of a year, for the chart (it draws a balance: the same points under another name). */
const stashHistory = (id: string, year: string) => stashesApi.history(id, year).then((points) => points.map((point) => ({ month: point.month, balance: point.saved })))

/**
 * One stash, under the header (its back button, name, what it holds and its buttons are in the page's `PageHeader`): how far it is as a bar; its facts, one to
 * a line; the notes; the saved amount over the year as a chart with a dashed line at the target; the history of what was added and taken out, newest first, a
 * click on a row opens it; and what its account has, with a warning when the stashes hold more than the account has. docs/stash-design.md, section 13.3.
 */
export default function StashDetail({
  detail,
  accounts,
  settings,
  now,
  reload,
  onEntry,
  onTransfer,
}: {
  detail: Detail
  accounts: Account[]
  settings: MoneySettings
  now: Today
  /** Read again when this changes. */
  reload: number
  onEntry: (entry: Entry) => void
  /** Open the transfer that came with an entry. */
  onTransfer: (id: string) => void
}) {
  const { t } = useI18n()
  const phone = useProfile() === 'phone'
  const rows = useOpenRow()
  const [month, setMonth] = useState(now.month)
  const { stash, entries } = detail
  const money = stashMoney(stash, settings.locale)
  const account = accountById(accounts, stash.accountId)

  /** "+50.00" in the success colour, "-40.00" in the danger colour. */
  const signed = (entry: Entry) => (entry.kind === 'ADD' ? { text: `+${money(entry.amount)}`, tone: 'is-income' } : { text: `-${money(entry.amount)}`, tone: 'is-expense' })
  const transferText = (entry: Entry) => (entry.otherAccount ? t(entry.kind === 'ADD' ? 'stashes.fromAccount' : 'stashes.toAccount', { account: entry.otherAccount }) : '')
  const amountCell = (entry: Entry) => {
    const view = signed(entry)
    return <span className={`cell-amount ${view.tone}`}>{view.text}</span>
  }

  // A desktop row, one value to a cell: the day, the note (or a dash), the transfer that came with it, the amount (signed), what the stash held after, then edit.
  function tableRow(entry: Entry) {
    return (
      <Row
        key={entry.id}
        data-entry={entry.id}
        onClick={(event) => {
          if ((event.target as Element).closest('a, button')) return
          onEntry(entry)
        }}
        actions={<IconAction icon="pencil" label={t('stashes.editEntry')} data-row-focus onClick={() => onEntry(entry)} />}
      >
        <span className="cell-sub line-date">{formatDate(entry.date, settings.locale)}</span>
        <div className="cell-name">
          <span className="card-name stash-note" title={entry.note ?? undefined}>
            {entry.note ?? '—'}
          </span>
        </div>
        <span className="cell-sub stash-transfer">
          {entry.transferId !== null && entry.otherAccount ? (
            <button type="button" className="stash-link" onClick={() => onTransfer(entry.transferId ?? '')}>
              {transferText(entry)}
            </button>
          ) : null}
        </span>
        {amountCell(entry)}
        <span className={cn('cell-sub-end stash-held', entry.savedAfter.startsWith('-') && 'is-below')}>{money(entry.savedAfter)}</span>
      </Row>
    )
  }

  // A phone row: the note (or a dash) and the amount, the transfer that came with it under the name. It opens to the day and what the stash held after, and
  // the buttons: edit, and the transfer.
  function tapRow(entry: Entry) {
    const key = String(entry.id)
    const actions: (PanelAction | false)[] = [
      { label: t('stashes.editEntry'), icon: 'pencil', onClick: () => onEntry(entry) },
      entry.transferId !== null && entry.otherAccount !== null && { label: transferText(entry), icon: 'sync', onClick: () => onTransfer(entry.transferId ?? '') },
    ]
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={entry.note ?? '—'}
        amount={amountCell(entry)}
        meta={entry.otherAccount ? transferText(entry) : undefined}
        facts={[
          { label: t('report.date'), value: formatDate(entry.date, settings.locale) },
          { label: t('stashes.savedAfter'), value: money(entry.savedAfter), tone: entry.savedAfter.startsWith('-') ? 'negative' : undefined },
        ]}
      >
        <PanelActions actions={actions} />
      </TapRow>
    )
  }

  const accountNode = account ? (
    <Link to={`/app/accounts/${account.id}`} title={accountLabel(account, t)}>
      {accountLabel(account, t)}
    </Link>
  ) : (
    stash.account
  )

  return (
    <>
      <StashBar saved={stash.saved} target={stash.target} large />
      <Facts items={stashFacts(stash, accountNode, money, settings.locale, t)} className="acct-facts" />
      {stash.notes ? <p className="acct-notes">{stash.notes}</p> : null}
      <Card className="acct-chart">
        <CardHeader>
          <CardAction>
            <YearNav month={month} thisMonth={now.month} onChange={setMonth} />
          </CardAction>
        </CardHeader>
        <BalanceChart
          accountId={stash.id}
          currency={stash.currency}
          month={month}
          reload={reload}
          onPick={setMonth}
          settings={settings}
          history={stashHistory}
          target={stash.target}
          label={t('stashes.savedLabel')}
          chartLabel={t('stashes.chart')}
        />
      </Card>
      <h2 className="debts-heading">{t('stashes.history')}</h2>
      {entries.length === 0 ? <p className="hint">{t('stashes.noEntries')}</p> : null}
      {entries.length > 0 && phone ? (
        <DayGroups rows={entries} dateOf={(entry) => entry.date} locale={settings.locale} today={now.date}>
          {tapRow}
        </DayGroups>
      ) : null}
      {entries.length > 0 && !phone ? (
        <Table
          tidy
          className="stash-history"
          heads={[t('report.date'), t('debts.note'), { label: '', className: 'head-transfer' }, { label: t('tx.amount'), className: 'is-end' }, { label: t('stashes.savedAfter'), className: 'is-end head-after' }, '']}
        >
          {entries.map(tableRow)}
        </Table>
      ) : null}
      {/* what its account has: the balance, what the stashes hold and what is free; red, with a warning, when they hold more than the account has */}
      <Facts
        className="acct-facts"
        items={[
          { label: account ? accountName(account, t) : stash.account, value: account ? money(account.balance) : null },
          { label: t('accounts.setAsideHead'), value: account ? money(account.setAside) : null },
          { label: t('accounts.free'), value: money(stash.free), tone: overBooked(stash.free) ? 'negative' : undefined },
        ]}
      />
      {overBooked(stash.free) ? <p className="stash-warn">{t('stashes.overBooked')}</p> : null}
    </>
  )
}
