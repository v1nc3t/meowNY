import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { categories as categoriesApi, type Category, type CategoryType } from '../api/categories'
import { debts } from '../api/debts'
import { errorText } from '../api/invoke'
import { transactions as transactionsApi, type Transaction } from '../api/transactions'
import { transfers as transfersApi } from '../api/transfers'
import { useAccounts } from '../hooks/useAccounts'
import { useAttachments } from '../hooks/useAttachments'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { arrivalText, chooseSide, dialogNext, transferCurrencies, transferSubmit, withArrival, type DialogState } from '../lib/accountDialogs'
import { rememberAccount, transferOptions } from '../lib/accountsView'
import {
  accountField,
  categoryChoices,
  changeTab,
  chooseAccount,
  entryOf,
  entryStart,
  entryTabs,
  nativeView,
  splitAmount,
  transactionBody,
  withInMain,
  type EntryContext,
  type EntryState,
  type EntryTab,
} from '../lib/entryDialog'
import { remindersChanged } from '../lib/reminders'
import { amountString } from '../lib/money'
import { equalDraft, isShared, toInput, type SplitState } from '../lib/split'
import type { Account } from '../api/accounts'
import AccountField from './AccountField'
import AddCategoryModal from './AddCategoryModal'
import AttachArea from './AttachArea'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import EntryTabs from './EntryTabs'
import Modal, { modalFormId } from './Modal'
import SplitEditor from './SplitEditor'
import TransferInputs from './TransferInputs'

/** What the opener may ask of a new entry. */
export type EntryInitial = {
  /** The tab it opens on; expense when left out (or when this dialog has no such tab: a transfer needs two accounts). */
  tab?: EntryTab
  /** The account the entry starts in (the account of an expense or an income, the "from" of a transfer). An archived or unknown one is not used. */
  accountId?: string
  /** What the amount field starts with, as typed ("12.50"): the interest of an account that earns some (docs/loans-deposits-design.md, section 5.6). */
  amount?: string
  /** What the name starts with ("interest"). */
  name?: string
  /** Opens the expense with the bill dialog on, and `friend` already on it (the debts pages). */
  split?: boolean
  friend?: { id: string; name: string }
}

/**
 * The dialog that adds a transaction: tabs for an expense, an income and a transfer (docs/accounts-design.md, sections 20.1 and 20.7). It reads
 * the accounts and the categories itself, saves itself and tells its opener: `onSaved` when something was saved (even when the dialog was closed
 * while it saved), `onClose` when it goes away (cancel, `Esc`, or after a save). With a `transaction` it changes that one: no tabs, on the tab
 * of its category's type. A transfer is changed in `TransferModal`.
 */
export default function EntryDialog({
  transaction = null,
  initial,
  today,
  onClose,
  onSaved,
}: {
  /** The transaction to change; none for a new entry. */
  transaction?: Transaction | null
  initial?: EntryInitial
  /** "YYYY-MM-DD", the date a new entry starts on. */
  today: string
  onClose: () => void
  onSaved: () => void
}) {
  const say = useToast()
  const accounts = useAccounts()
  const [categories, setCategories] = useState<Category[] | null>(null)
  useEffect(() => {
    let cancelled = false
    void categoriesApi.list().then(
      (list) => {
        if (!cancelled) setCategories(list)
      },
      (failure) => {
        if (cancelled) return
        say(errorText(failure), 'error')
        onClose()
      },
    )
    return () => {
      cancelled = true
    }
    // read once, when the dialog opens
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  if (!accounts || !categories) return null
  return <EntryForm transaction={transaction} initial={initial} accounts={accounts} loaded={categories} today={today} onClose={onClose} onSaved={onSaved} />
}

function EntryForm({
  transaction,
  initial,
  accounts,
  loaded,
  today,
  onClose,
  onSaved,
}: {
  transaction: Transaction | null
  initial?: EntryInitial
  accounts: Account[]
  loaded: Category[]
  today: string
  onClose: () => void
  onSaved: () => void
}) {
  const t = useT()
  const say = useToast()
  const [categories, setCategories] = useState(loaded)
  // the transaction a first try made when its bill then failed: the next try changes that one (the same client id would give back the old
  // row, with the old amount, and the bill would be shared on that). From then on the dialog is a change of that row: no tabs.
  const [savedId, setSavedId] = useState<string | null>(null)
  const clientUuid = useRef(crypto.randomUUID())
  const tabs = entryTabs(accounts, { editing: transaction !== null || savedId !== null, transfer: true })
  const context: EntryContext = { accounts, tabs, today, accountId: initial?.accountId }
  const [entry, setEntry] = useState<EntryState>(() => {
    if (transaction) return entryOf(transaction, loaded, accounts)
    const start = { ...entryStart(context, initial?.tab), ...(initial?.amount ? { amount: initial.amount } : {}), ...(initial?.name ? { name: initial.name } : {}) }
    if (!initial?.split || start.tab !== 'EXPENSE') return start
    const friends = initial.friend ? [{ personId: initial.friend.id, name: initial.friend.name }] : []
    return { ...start, split: { on: true, draft: equalDraft(friends), ready: false, touched: false } }
  })
  const [state, setState] = useState<DialogState>('EDITING')
  const [addingCategory, setAddingCategory] = useState(false)
  // The documents area is behind a switch, as the split is. It is open when the transaction already has documents (they are listed
  // as soon as they are loaded) or files were chosen for a new one, and then it cannot be closed on them.
  const [docsOn, setDocsOn] = useState(false)
  // The documents of the transaction being changed are stored and linked at once; those of one being made wait for it to be saved.
  const attachments = useAttachments(transaction ? { kind: 'transaction', id: transaction.id } : null)
  // the dialog may be closed while it saves: what the answer brings then is not told to a dialog that is no more
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])

  const set = (patch: Partial<EntryState>) => setEntry((current) => ({ ...current, ...patch }))
  const setSplit = (split: SplitState) => set({ split })
  const onSplitReady = useCallback(
    (ready: boolean) => setEntry((current) => (current.split.ready === ready ? current : { ...current, split: { ...current.split, ready } })),
    [],
  )

  function pick(tab: EntryTab) {
    const next = changeTab(entry, tab, context)
    if (next === entry) return
    // a transfer has no documents
    if (tab === 'TRANSFER') attachments.reset()
    setEntry(next)
  }

  const type: CategoryType = entry.tab === 'INCOME' ? 'INCOME' : 'EXPENSE'
  const options = categoryChoices(categories, type, transaction ? { id: transaction.categoryId, label: t('tx.deletedCategory') } : undefined)
  // an income needs a category of its own: where there is none the dialog says so and cannot be saved
  const noIncomeCategory = entry.tab === 'INCOME' && options.length === 0
  const hasDocuments = attachments.linked.length > 0 || attachments.pending.length > 0 || attachments.working !== ''
  const showDocuments = docsOn || hasDocuments
  const hadSplit = transaction ? isShared(transaction) : false
  // with an account in another currency there are two amounts; what a bill is split over, and what counts, is the one in the app's currency
  const native = nativeView(accounts, entry, transaction)
  const billAmount = splitAmount(accounts, entry, transaction)
  const rescaled = hadSplit && !entry.split.touched && amountString(billAmount, false) !== transaction?.amount
  const account = accountField(accounts, type, entry.account, transaction?.accountId)
  const currencies = transferCurrencies(accounts, entry)

  /** Sends one request; an error gives the fields back, an answer closes the dialog and tells the opener. */
  async function send(work: () => Promise<void>, remember: string) {
    setState((current) => dialogNext(current, 'send'))
    try {
      await work()
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    // the next new row starts in this account
    rememberAccount(remember)
    onSaved()
    if (alive.current) onClose()
  }

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (state !== 'EDITING') return
    if (entry.tab === 'TRANSFER') {
      const step = transferSubmit(state, null, { from: entry.from, to: entry.to, amount: entry.amount, date: entry.date, note: entry.text, arrives: entry.arrives, arrivesTouched: entry.arrivesTouched }, accounts)
      if (step.do === 'refuse') say(t(step.error), 'error')
      else if (step.do === 'send' && 'create' in step.request) {
        const request = step.request.create
        await send(() => transfersApi.create(request).then(() => undefined), entry.from)
      }
      return
    }
    const built = transactionBody(entry, accounts, transaction)
    if ('error' in built) {
      say(t(built.error, built.vars), 'error')
      return
    }
    // A shared bill is saved when its numbers add up; one that was shared and is left alone is scaled by the app to a new amount.
    const wantsSplit = entry.tab === 'EXPENSE' && entry.split.on
    const sendSplit = wantsSplit && (entry.split.touched || !hadSplit)
    const input = sendSplit ? toInput(entry.split.draft) : null
    if (sendSplit && (!entry.split.ready || !input)) {
      say(t('split.notComplete'), 'error')
      return
    }
    // what could not be attached to a new transaction: it is saved all the same, and the toast names the files
    let notAttached: string[] = []
    await send(async () => {
      if (transaction) {
        await transactionsApi.update(transaction.id, built.body)
        if (input) await debts.split(transaction.id, input)
        else if (hadSplit && !wantsSplit) await debts.unsplit(transaction.id)
      } else {
        // creating again with the same client id gives back the same transaction, so a failed split can be tried again
        const created = savedId ? await transactionsApi.update(savedId, built.body) : await transactionsApi.create({ ...built.body, clientUuid: clientUuid.current })
        setSavedId(created.id)
        if (input) await debts.split(created.id, input)
        notAttached = await attachments.commit({ kind: 'transaction', id: created.id })
      }
      if (notAttached.length > 0) say(t('docs.attachFailed', { names: notAttached.join(', ') }), 'warning')
      // a shared bill changes what friends owe, and so the reminders
      if (input) remindersChanged()
    }, built.remember)
  }

  return (
    <>
      <Modal title={transaction ? t('tx.editTitle') : t('tx.addTitle')} pending={state === 'SAVING' || noIncomeCategory} onClose={onClose}>
        <EntryTabs tabs={tabs} value={entry.tab} label={t('tx.addTitle')} onChange={pick}>
          <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
            {entry.tab === 'TRANSFER' ? (
              <TransferInputs
                from={entry.from}
                to={entry.to}
                amount={entry.amount}
                date={entry.date}
                note={entry.text}
                options={transferOptions(accounts, t)}
                today={today}
                amountCode={currencies.amount}
                arrives={currencies.arrives ? { code: currencies.arrives, value: arrivalText(accounts, entry) } : null}
                onChoose={(side, id) => setEntry((current) => ({ ...current, ...chooseSide(current, side, id) }))}
                onChange={({ note, arrives, ...rest }) =>
                  setEntry((current) => ({ ...(arrives !== undefined ? withArrival(current, arrives) : current), ...rest, ...(note !== undefined ? { text: note } : {}) }))
                }
              />
            ) : (
              <>
                <label>
                  {t('common.name')}
                  <input name="name" required maxLength={50} value={entry.name} onChange={(event) => set({ name: event.target.value })} autoComplete="off" />
                </label>
                <label>
                  {native ? t('entry.amountIn', { code: native.code }) : t('tx.amount')}
                  <input name="amount" required inputMode="decimal" value={entry.amount} onChange={(event) => set({ amount: event.target.value })} autoComplete="off" />
                </label>
                {native ? (
                  <label>
                    {t('entry.inMain', { code: native.mainCode })}
                    <input name="inMain" required inputMode="decimal" value={native.inMain} onChange={(event) => setEntry((current) => withInMain(current, event.target.value))} autoComplete="off" />
                  </label>
                ) : null}
                {noIncomeCategory ? (
                  <p className="hint">
                    {t('entry.noIncomeCategory')} <Link to="/app/categories?tab=income" onClick={() => onClose()}>{t('entry.addIncomeCategory')}</Link>
                  </p>
                ) : (
                  <Dropdown
                    label={t('common.category')}
                    value={entry.categoryId}
                    options={options}
                    onChange={(categoryId) => set({ categoryId })}
                    placeholder={t('tx.chooseCategory')}
                    action={{ label: t('categories.add'), onSelect: () => setAddingCategory(true) }}
                  />
                )}
                <AccountField
                  accounts={accounts}
                  value={account.value}
                  onChange={(chosen) => setEntry((current) => chooseAccount(accounts, current, transaction, chosen))}
                  into={entry.tab === 'INCOME'}
                />
                <div className="tx-date-group">
                  <span>{t('tx.paymentDate')}</span>
                  <DatePicker kind="date" value={entry.date} label={t('tx.paymentDate')} today={today} onChange={(date) => set({ date })} />
                </div>
                {entry.tab === 'EXPENSE' ? (
                  <label>
                    <input type="checkbox" checked={entry.split.on} onChange={(event) => setSplit({ ...entry.split, on: event.target.checked })} />
                    {t('split.switch')}
                  </label>
                ) : null}
                {entry.tab === 'EXPENSE' && entry.split.on ? (
                  rescaled ? (
                    <p className="hint">{t('split.rescaled')}</p>
                  ) : (
                    <SplitEditor amount={billAmount} draft={entry.split.draft} onChange={(draft) => setSplit({ ...entry.split, draft, touched: true })} onReady={onSplitReady} />
                  )
                ) : null}
                <label>
                  {t('common.description')}
                  <textarea name="description" rows={3} maxLength={255} placeholder={t('common.optional')} value={entry.text} onChange={(event) => set({ text: event.target.value })} autoComplete="off" />
                </label>
                <label>
                  <input type="checkbox" checked={showDocuments} disabled={hasDocuments} onChange={(event) => setDocsOn(event.target.checked)} />
                  {t('docs.attachSwitch')}
                </label>
              </>
            )}
          </form>
          {entry.tab !== 'TRANSFER' && showDocuments ? <AttachArea bare attachments={attachments} /> : null}
        </EntryTabs>
      </Modal>
      {addingCategory ? (
        <AddCategoryModal
          type={type}
          onClose={() => setAddingCategory(false)}
          onCreated={(category) => {
            setCategories((current) => [...current, category])
            // a category of the other type is made, and is not on this tab's list
            if (category.type === type) set({ categoryId: category.id })
            setAddingCategory(false)
          }}
        />
      ) : null}
    </>
  )
}
