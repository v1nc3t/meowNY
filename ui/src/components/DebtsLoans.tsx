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
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'
import { accountName } from '../lib/accountsView'
import { liabilities, payoffWords, rateWords, totalOwed } from '../lib/loansView'
import { formatMoney, moneyIn } from '../lib/money'
import { tabPanelProps } from '../lib/tabs'
import AccountModal from './AccountModal'
import LoanPayModal from './LoanPayModal'
import { Row, Table, TapRow } from './RowParts'
import Button from './ui/Button'
import { buttonClass } from './ui/cn'

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

  const payButton = (account: Account) =>
    account.archived ? null : (
      <Button variant="outline" size="sm" className="row-primary" onClick={() => setDialog({ kind: 'pay', liability: account })}>
        {t('loans.pay')}
      </Button>
    )

  // A desktop row: the name with its kind under it, what is owed, the rate, the installment, the payoff, then pay. Under 44em the columns after the
  // amount are one line under the name (`loan-line`), as the accounts do. The name is the link for the keyboard (j and k stop on it).
  function tableRow(account: Account) {
    const small = [
      kindText(account),
      account.archived ? t('accounts.archived') : '',
      rateWords(account, t),
      account.installment === null ? '' : `${t('accounts.installment')} ${installmentOf(account)}`,
      payoffOf(account) === '' ? '' : `${t('loans.payoff')} ${payoffOf(account)}`,
    ]
    return (
      <Row key={account.id} data-archived={account.archived || undefined} onClick={(event) => open(event, account)} actions={payButton(account)}>
        <div className="cell-name">
          <span className="acct-name">
            <Link className="card-name" to={link(account)} title={accountName(account, t)} data-row-focus>
              {accountName(account, t)}
            </Link>
            <span className="row-mark loan-kind">{account.archived ? `${kindText(account)} · ${t('accounts.archived')}` : kindText(account)}</span>
          </span>
        </div>
        <span className="cell-amount">{owedOf(account)}</span>
        <span className="cell-sub loan-col">{rateWords(account, t)}</span>
        <span className="cell-sub-end loan-col is-installment">{installmentOf(account)}</span>
        <span className="cell-sub loan-col">{payoffOf(account)}</span>
        <span className="cell-sub loan-line">{small.filter(Boolean).join(' · ')}</span>
      </Row>
    )
  }

  // A phone row: the name and what is owed, the kind and the rate under them. It opens to the installment and the payoff, and a labelled button for
  // everything that can be done.
  function tapRow(account: Account) {
    const more = [account.installment === null ? '' : `${t('accounts.installment')} ${installmentOf(account)}`, payoffOf(account) === '' ? '' : `${t('loans.payoff')} ${payoffOf(account)}`].filter(Boolean)
    return (
      <TapRow
        key={account.id}
        id={rows.panelId(account.id)}
        data-archived={account.archived || undefined}
        open={rows.isOpen(account.id)}
        onToggle={() => rows.toggle(account.id)}
        name={
          <>
            {accountName(account, t)}
            {account.archived ? <span className="row-mark">{t('accounts.archived')}</span> : null}
          </>
        }
        amount={<span className="cell-amount">{owedOf(account)}</span>}
        sub={
          <>
            <span>{kindText(account)}</span>
            {rateWords(account, t) ? <span>{rateWords(account, t)}</span> : null}
          </>
        }
      >
        {more.length > 0 ? <p className="hint">{more.join(' · ')}</p> : null}
        <div className="row-panel-actions">
          {account.archived ? null : (
            <Button variant="outline" onClick={() => setDialog({ kind: 'pay', liability: account })}>
              {t('loans.pay')}
            </Button>
          )}
          <Link to={link(account)} className={buttonClass('outline')}>
            {t('accounts.open')}
          </Link>
        </div>
      </TapRow>
    )
  }

  return (
    <section className="tx-page debts-page">
      <div className="tx-title one-line">
        <h1>{t('debts.title')}</h1>
        <div className="tx-actions debts-actions">
          <Button onClick={() => setDialog({ kind: 'new' })} {...tip('new', t('loans.new'))}>
            {t('loans.new')}
          </Button>
        </div>
      </div>
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
          {list.length > 0 ? (
            <p className="acct-total">
              <span className="dash-stat-label">{t('loans.totalOwed')}</span> <b>{money(totalOwed(accounts))}</b>
            </p>
          ) : null}
          {list.length > 0 && phone ? <ul className="card-list">{list.map(tapRow)}</ul> : null}
          {list.length > 0 && !phone ? (
            <Table
              className="loans-table"
              heads={[
                t('common.name'),
                { label: t('accounts.owed'), className: 'is-end' },
                t('accounts.rate'),
                { label: t('accounts.installment'), className: 'is-end' },
                t('loans.payoff'),
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
