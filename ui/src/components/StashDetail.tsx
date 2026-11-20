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
import { canAdd, dayWords, nextLine, nextText, overBooked, progress, stashMoney } from '../lib/stashView'
import BalanceChart from './BalanceChart'
import Icon from './NavIcon'
import { DayGroups, IconAction, Row, Table, TapRow } from './RowParts'
import { StashBar } from './StashParts'
import YearNav from './YearNav'
import Button from './ui/Button'
import { Card, CardAction, CardHeader } from './ui/Card'
import { buttonClass, cn } from './ui/cn'

/** What a stash held at the end of each month of a year, for the chart (it draws a balance: the same points under another name). */
const stashHistory = (id: string, year: string) => stashesApi.history(id, year).then((points) => points.map((point) => ({ month: point.month, balance: point.saved })))

/**
 * One stash: back, its name and account, and what can be done with it (edit, archive or restore, delete); a large block that says how far it is; the
 * notes; the saved amount over the year as a chart with a dashed line at the target; the history of what was added and taken out, newest first, a click on
 * a row opens it; and what its account has free, with a warning when the stashes hold more than the account has. docs/stash-design.md, section 13.3.
 */
export default function StashDetail({
  detail,
  accounts,
  settings,
  now,
  reload,
  onEdit,
  onArchive,
  onRestore,
  onDelete,
  onEntry,
  onTransfer,
}: {
  detail: Detail
  accounts: Account[]
  settings: MoneySettings
  now: Today
  /** Read again when this changes. */
  reload: number
  onEdit: () => void
  onArchive: () => void
  onRestore: () => void
  onDelete: () => void
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
  const shown = progress(stash.saved, stash.target)
  const line = nextLine(stash)
  const words = line ? nextText(line, money, t, settings.locale) : null
  const below = stash.saved.startsWith('-')

  /** "+50.00" in the success colour, "-40.00" in the danger colour. */
  const signed = (entry: Entry) => (entry.kind === 'ADD' ? { text: `+${money(entry.amount)}`, tone: 'is-income' } : { text: `-${money(entry.amount)}`, tone: 'is-expense' })
  const mark = (entry: Entry) => (
    <span className={cn('stash-mark', entry.kind === 'ADD' ? 'is-income' : 'is-expense')}>
      <span aria-hidden="true">{entry.kind === 'ADD' ? '+' : '−'}</span>
      <span className="sr-only">{t(`stashes.kind.${entry.kind}`)}</span>
    </span>
  )
  const transferText = (entry: Entry) => (entry.otherAccount ? t(entry.kind === 'ADD' ? 'stashes.fromAccount' : 'stashes.toAccount', { account: entry.otherAccount }) : '')
  const amountCell = (entry: Entry) => {
    const view = signed(entry)
    return <span className={`cell-amount ${view.tone}`}>{view.text}</span>
  }

  // A desktop row: the day, + or -, the note (or a dash), the transfer that came with it, the amount, what the stash held after, then edit.
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
        {mark(entry)}
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
        <span className={cn('cell-sub-end', entry.savedAfter.startsWith('-') && 'is-below')}>{money(entry.savedAfter)}</span>
      </Row>
    )
  }

  // A phone row: the note (or a dash) and the amount, what the stash held after under them. It opens to a labelled button for what can be done.
  function tapRow(entry: Entry) {
    const key = String(entry.id)
    return (
      <TapRow
        key={key}
        id={rows.panelId(key)}
        open={rows.isOpen(key)}
        onToggle={() => rows.toggle(key)}
        name={
          <>
            {mark(entry)} {entry.note ?? '—'}
          </>
        }
        amount={amountCell(entry)}
        sub={
          <>
            {entry.otherAccount ? <span>{transferText(entry)}</span> : null}
            <span>{`${t('stashes.savedAfter')} ${money(entry.savedAfter)}`}</span>
          </>
        }
      >
        <div className="row-panel-actions">
          <Button variant="outline" onClick={() => onEntry(entry)}>
            <Icon name="pencil" className="btn-icon" />
            {t('stashes.editEntry')}
          </Button>
          {entry.transferId !== null && entry.otherAccount ? (
            <Button variant="outline" onClick={() => onTransfer(entry.transferId ?? '')}>
              {transferText(entry)}
            </Button>
          ) : null}
        </div>
      </TapRow>
    )
  }

  return (
    <>
      <div className="debts-name stash-actions-row">
        <Link to="/app/stashes" className={buttonClass('outline', 'sm')}>
          {t('common.back')}
        </Link>
        {stash.archived ? (
          <Button variant="outline" size="sm" onClick={onRestore}>
            {t('stashes.restore')}
          </Button>
        ) : (
          <>
            <Button variant="outline" size="sm" onClick={onEdit}>
              {t('common.edit')}
            </Button>
            <Button variant="outline" size="sm" onClick={onArchive}>
              <Icon name="archive" className="btn-icon" />
              {t('stashes.archive')}
            </Button>
          </>
        )}
        <Button variant="outline" size="sm" className="row-danger" onClick={onDelete}>
          <Icon name="trash" className="btn-icon" />
          {t('common.delete')}
        </Button>
      </div>
      <h2 className="debts-heading stash-heading">{stash.name}</h2>
      <p className="hint stash-account-line">
        {account ? (
          <Link to={`/app/accounts/${account.id}`} title={accountLabel(account, t)}>
            {accountLabel(account, t)}
          </Link>
        ) : (
          stash.account
        )}
        {stash.archived ? ` · ${t('accounts.archived')}` : ''}
      </p>
      <Card className="stash-block">
        <div className="stash-block-numbers">
          <span className="stash-block-main">
            <span className="dash-stat-label">{t('stashes.savedLabel')}</span>
            <span className={cn('stash-big', below && 'is-below')}>{money(stash.saved)}</span>
          </span>
          {stash.target !== null ? (
            <span className="stash-block-main">
              <span className="dash-stat-label">{t('stashes.target')}</span>
              <span className="stash-big">{money(stash.target)}</span>
            </span>
          ) : null}
          {stash.percent !== null ? (
            <span className={cn('stash-percent', shown.reached && 'is-reached')}>{t('stashes.percent', { percent: stash.percent })}</span>
          ) : null}
        </div>
        <StashBar saved={stash.saved} target={stash.target} large />
        <div className="stash-block-facts">
          {words ? <span className={cn('stash-next', words.warning && 'is-late', shown.reached && 'is-reached')}>{words.text}</span> : null}
          {stash.targetDate ? (
            <span className="stash-next">
              {t('stashes.targetDate')} {dayWords(stash.targetDate, settings.locale)}
            </span>
          ) : null}
        </div>
        {stash.notes ? <p className="acct-notes">{stash.notes}</p> : null}
        {stash.accountArchived && !stash.archived ? <p className="hint">{t('stashes.accountArchivedHint')}</p> : null}
        {stash.archived && !stash.accountArchived && !canAdd(stash) ? <p className="hint">{t('stashes.archivedHint')}</p> : null}
      </Card>
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
          className="stash-history"
          heads={[t('report.date'), '', t('debts.note'), '', { label: t('tx.amount'), className: 'is-end' }, { label: t('stashes.savedAfter'), className: 'is-end' }, '']}
        >
          {entries.map(tableRow)}
        </Table>
      ) : null}
      <p className={cn('stash-free-line', overBooked(stash.free) && 'is-over')}>
        {account ? accountName(account, t) : stash.account}: {money(account?.balance ?? stash.free)}
        {account ? ` · ${t('stashes.setAside', { amount: money(account.setAside) })}` : ''} · <span className="stash-free">{t('stashes.free', { amount: money(stash.free) })}</span>
        {overBooked(stash.free) ? <span className="stash-warn"> {t('stashes.overBooked')}</span> : null}
      </p>
    </>
  )
}
