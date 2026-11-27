import { useEffect, useState, type MouseEvent, type ReactNode } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { accounts as accountsApi, type Account } from '../api/accounts'
import { errorText } from '../api/invoke'
import type { Today } from '../api/today'
import { useProfile } from '../hooks/profile'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useOpenRow } from '../hooks/useOpenRow'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'
import { accountName } from '../lib/accountsView'
import { liabilities, loanFacts, payoffWords, totalOwed } from '../lib/loansView'
import { formatMoney, moneyIn } from '../lib/money'
import { tabPanelProps } from '../lib/tabs'
import PageHeader from './PageHeader'
import AccountModal from './AccountModal'
import LoanPayModal from './LoanPayModal'
import Facts from './Facts'
import { IconAction, ListGroup, NameMark, PanelActions, Row, RowMore, Table, TapRow, type PanelAction } from './RowParts'
import { Badge } from './ui/badge'

/** The dialog that is open: a new debt, or the payment of one. */
type Dialog = { kind: 'new' } | { kind: 'pay'; liability: Account } | null

/**
 * The debts page's second tab (docs/loans-deposits-design.md, section 10): the accounts you owe on, a credit, a loan or a mortgage, with what is owed
 * in all, a button for a new one and a list, the archived ones last and dimmed. Each row names the debt (a link to its page), its kind, what is
 * owed, the rate, the installment and when it is paid off, and has the button that pays it. The page's own title row and tabs (`tabs`) are drawn here
 * because the friends tab has its own buttons in the title row.
 */
export default function DebtsLoans({ settings, now, tabs }: { settings: MoneySettings; now: Today; tabs: ReactNode }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  const rows = useOpenRow()
  const say = useToast()
  const [accounts, setAccounts] = useState<Account[] | null>(null)
  const [reload, setReload] = useState(0)
  const refresh = () => setReload((value) => value + 1)
  const [dialog, setDialog] = useState<Dialog>(null)
  const money = (amount: string) => formatMoney(amount, settings.currency, settings.locale)

  useEffect(() => {
    let cancelled = false
    void accountsApi.list().then(
      (list) => {
        if (!cancelled) setAccounts(list)
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [reload, say])

  // Keyboard: n adds a debt, j and k move between rows.
  useShortcut('new', () => setDialog({ kind: 'new' }))
  useRowNavigation()

  const list = liabilities(accounts ?? [])
  const link = (account: Account) => `/app/accounts/${account.id}`
  const owedOf = (account: Account) => moneyIn(account.currency, settings.locale)(account.liability?.owed ?? '0.00')
  const kindText = (account: Account) => t(`accounts.kind.${account.kind}`)
  const payoffOf = (account: Account) => payoffWords(account.liability?.payoff, settings.locale, t)
  const installmentOf = (account: Account) => (account.installment === null ? '' : moneyIn(account.currency, settings.locale)(account.installment))

  /** A click anywhere on a desktop row opens the debt, as its name does. Not one on a link or a button, or one that ends a text selection. */
  function open(event: MouseEvent<HTMLElement>, account: Account) {
    if ((event.target as Element).closest('a, button, input, label') || window.getSelection()?.isCollapsed === false) return
    navigate(link(account))
  }

  /** What a debt owes and costs, one to a line, in its own currency. */
  const factsOf = (account: Account) => loanFacts(account, moneyIn(account.currency, settings.locale), settings.locale, t)

  /** What can be done with a debt, in a desktop row's "more" menu and the open row of a phone: pay it (not an archived one) and open it. */
  const actionsOf = (account: Account): PanelAction[] => [
    ...(account.archived ? [] : [{ label: t('loans.pay'), icon: 'check', onClick: () => setDialog({ kind: 'pay', liability: account }) }]),
    { label: t('accounts.open'), icon: 'play', to: link(account) },
  ]

  // A desktop row: the name (a debt that was archived says so), its kind, what is owed, the installment, the payoff, then pay as an icon button, the
  // "more" menu (pay, open) and the chevron that opens the rest (the rate, a month of interest). The name is the link for the keyboard (j and k stop on it).
  function tableRow(account: Account) {
    return (
      <Row
        key={account.id}
        rows={rows}
        id={account.id}
        data-archived={account.archived || undefined}
        onClick={(event) => open(event, account)}
        actions={
          <>
            {account.archived ? <span className="icon-slot" /> : <IconAction icon="check" label={`${t('loans.pay')}: ${accountName(account, t)}`} className="icon-btn rec-pay" onClick={() => setDialog({ kind: 'pay', liability: account })} />}
            <RowMore name={accountName(account, t)} actions={actionsOf(account)} />
          </>
        }
        panel={<Facts items={factsOf(account)} wide />}
      >
        <div className="cell-name">
          <Link className="card-name" to={link(account)} title={accountName(account, t)} data-row-focus>
            {accountName(account, t)}
          </Link>
          {account.archived ? <span className="row-mark">{t('accounts.archived')}</span> : null}
        </div>
        <span className="cell-sub loan-kind">{kindText(account)}</span>
        <span className="cell-amount">{owedOf(account)}</span>
        <span className="cell-sub-end loan-col is-installment">{installmentOf(account)}</span>
        <span className="cell-sub-end loan-col">{payoffOf(account)}</span>
      </Row>
    )
  }

  // A phone row: the name (an archived debt says so) and what is owed, the kind under the name. It opens to the rest, and a button for everything
  // that can be done.
  function tapRow(account: Account) {
    return (
      <TapRow
        key={account.id}
        id={rows.panelId(account.id)}
        data-archived={account.archived || undefined}
        open={rows.isOpen(account.id)}
        onToggle={() => rows.toggle(account.id)}
        name={<NameMark mark={account.archived ? <Badge variant="secondary">{t('accounts.archived')}</Badge> : null}>{accountName(account, t)}</NameMark>}
        amount={<span className="cell-amount">{owedOf(account)}</span>}
        meta={kindText(account)}
        facts={factsOf(account)}
      >
        <PanelActions actions={actionsOf(account)} />
      </TapRow>
    )
  }

  return (
    <section className="tx-page debts-page">
      <PageHeader
        title={t('debts.title')}
        actions={[{ label: t('loans.new'), icon: 'add', primary: true, shortcut: 'new', onClick: () => setDialog({ kind: 'new' }) }]}
        aside={
          accounts && list.length > 0 ? (
            <>
              {t('loans.totalOwed')} <b>{money(totalOwed(accounts))}</b>
            </>
          ) : null
        }
      />
      {tabs}
      {dialog?.kind === 'new' ? (
        <AccountModal
          account={null}
          group="liability"
          today={now.date}
          mainCurrency={settings.currency}
          groups={[]}
          onClose={() => setDialog(null)}
          onSaved={() => {
            // the dialog may have been closed while it saved, and another opened since: only its own is closed
            setDialog((current) => (current === dialog ? null : current))
            refresh()
          }}
        />
      ) : null}
      {dialog?.kind === 'pay' && accounts ? (
        <LoanPayModal
          key={`pay:${dialog.liability.id}`}
          liability={dialog.liability}
          accounts={accounts}
          locale={settings.locale}
          today={now.date}
          onClose={() => setDialog(null)}
          onSaved={() => {
            setDialog((current) => (current === dialog ? null : current))
            refresh()
          }}
        />
      ) : null}
      {accounts ? (
        <div className="tx-pane wide pane-stack" {...tabPanelProps('debts')}>
          {list.length === 0 ? <p className="hint">{t('loans.none')}</p> : null}
          {list.length > 0 && phone ? <ListGroup>{list.map(tapRow)}</ListGroup> : null}
          {list.length > 0 && !phone ? (
            <Table
              className="loans-table"
              tidy
              heads={[
                t('common.name'),
                { label: t('common.type'), className: 'head-kind' },
                { label: t('accounts.owed'), className: 'is-end' },
                { label: t('accounts.installment'), className: 'is-end head-installment' },
                { label: t('loans.payoff'), className: 'is-end' },
                '',
              ]}
            >
              {list.map(tableRow)}
            </Table>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}
