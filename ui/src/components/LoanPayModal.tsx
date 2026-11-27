import { useEffect, useState, type FormEvent } from 'react'
import { accounts as accountsApi, type Account } from '../api/accounts'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { dialogNext, type DialogState } from '../lib/accountDialogs'
import { accountLabel, accountName } from '../lib/accountsView'
import { categoryChoices } from '../lib/entryDialog'
import { checkPay, PAY_NOTE_MAX, payAfter, payFromChoices, payPrincipal, payStart, paySubmit, withAmount, withInterest, type PayFields } from '../lib/loanPay'
import { fromMinor, moneyIn } from '../lib/money'
import { saveChoice, storedId } from '../lib/sticky'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import { FactStrip } from './Facts'
import Modal, { modalFormId } from './Modal'

/** Where the account a payment came from, and its category, are kept for the next payment. */
const LAST_FROM = 'loans.from'
const LAST_CATEGORY = 'loans.category'

/**
 * Pays a credit, a loan or a mortgage (docs/loans-deposits-design.md, sections 5.4 and 5.6): one payment, split in two by the app. The interest part
 * is an expense from the paying account, in a category, and the rest, the principal, a transfer that lowers the debt. It opens with the account the last
 * payment came from (else the main account), the installment as the amount, a month of interest as the interest (never more than the amount, and it
 * follows the amount until it is typed), the last category used, and today. Under the amounts: a strip with the principal, what is still owed after it, and the state of the debt. Done is
 * off while the amount is wrong, the interest is above the amount, or there is interest and no category. The states are those of
 * docs/stash-design.md, section 5.3: errors go back to editing with a toast, and a second submit while saving is ignored. The core checks it all again.
 */
export default function LoanPayModal({
  liability,
  accounts,
  locale,
  today,
  onClose,
  onSaved,
}: {
  /** The account that is paid: a credit, a loan or a mortgage that is not archived. */
  liability: Account
  /** Every account: the money comes from one of its assets that has the debt's currency. */
  accounts: Account[]
  locale: string
  today: string
  onClose: () => void
  /** Told when the payment was made, even if the dialog was closed while it saved, so the page can read its list again. */
  onSaved: () => void
}) {
  const say = useToast()
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
  if (!categories) return null
  return <PayForm liability={liability} accounts={accounts} categories={categories} locale={locale} today={today} onClose={onClose} onSaved={onSaved} />
}

function PayForm({
  liability,
  accounts,
  categories,
  locale,
  today,
  onClose,
  onSaved,
}: {
  liability: Account
  accounts: Account[]
  categories: Category[]
  locale: string
  today: string
  onClose: () => void
  onSaved: () => void
}) {
  const t = useT()
  const say = useToast()
  const choices = payFromChoices(accounts, liability)
  const [fields, setFields] = useState<PayFields>(() => payStart(liability, choices, categories, { from: storedId(LAST_FROM), category: storedId(LAST_CATEGORY) }, today))
  const [state, setState] = useState<DialogState>('EDITING')
  const set = (patch: Partial<PayFields>) => setFields((current) => ({ ...current, ...patch }))

  const money = moneyIn(liability.currency, locale)
  const check = checkPay(fields)
  // the strip under the amounts is there while only the category is missing; with the interest over the payment it has nothing to say
  const interestOver = !check.ok && check.reason === 'overAmount'
  const principal = payPrincipal(fields)
  const after = principal === null ? null : payAfter(liability.balance, principal)
  const preview = !interestOver && principal !== null && after ? { principal, owed: after.owed, state: after.state } : null
  const none = <span className="cell-none">—</span>
  // the category is asked for only when there is interest to book
  const interestless = check.ok && check.interest === 0n
  const name = accountName(liability, t)
  const code = liability.foreign ? liability.currency.toUpperCase() : null

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = paySubmit(state, liability.id, fields)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    setState((current) => dialogNext(current, 'send'))
    try {
      await accountsApi.pay(step.request)
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    // the next payment starts from the same account, and the same category once there was interest to book
    saveChoice(LAST_FROM, fields.from)
    if (step.request.categoryId) saveChoice(LAST_CATEGORY, step.request.categoryId)
    say(t('loans.paidToast', { amount: money(step.request.amount), name }), 'success')
    onSaved()
  }

  return (
    <Modal title={t('loans.payTitle', { name })} doneLabel={t('loans.pay')} pending={state === 'SAVING' || !check.ok} onClose={onClose}>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <Dropdown label={t('loans.from')} value={fields.from} options={choices.map((account) => ({ value: account.id, label: accountLabel(account, t) }))} onChange={(from) => set({ from })} />
        <label>
          {code ? t('entry.amountIn', { code }) : t('tx.amount')}
          <input
            name="amount"
            required
            inputMode="decimal"
            autoComplete="off"
            placeholder="0.00"
            data-autofocus
            aria-invalid={(!check.ok && check.reason === 'amount' && fields.amount !== '') || undefined}
            value={fields.amount}
            onChange={(event) => setFields((current) => withAmount(current, liability, event.target.value))}
          />
        </label>
        <label>
          {code ? t('accounts.labelIn', { label: t('loans.interest'), code }) : t('loans.interest')}
          <input
            name="interest"
            inputMode="decimal"
            autoComplete="off"
            placeholder="0.00"
            aria-invalid={(!check.ok && (check.reason === 'interest' || interestOver)) || undefined}
            value={fields.interest}
            onChange={(event) => setFields((current) => withInterest(current, event.target.value))}
          />
        </label>
        {/* always there, so the dialog does not change height as the amounts become right and wrong */}
        <div role="status">
          <FactStrip
            items={[
              { label: t('loans.principal'), value: preview ? money(fromMinor(preview.principal)) : none },
              { label: t('loans.owedAfter'), value: preview ? money(fromMinor(preview.owed)) : none },
              { label: t('loans.stateLabel'), value: preview ? t(`loans.state.${preview.state}`) : none },
            ]}
          />
          <p className="stash-after is-broken">{interestOver ? t('loans.interestOver') : ''}</p>
        </div>
        {interestless ? null : (
          <Dropdown
            label={t('loans.category')}
            value={fields.category}
            options={categoryChoices(categories, 'EXPENSE')}
            placeholder={t('tx.chooseCategory')}
            onChange={(category) => set({ category })}
          />
        )}
        <div className="tx-date-group">
          <span>{t('report.date')}</span>
          <DatePicker kind="date" value={fields.date} label={t('report.date')} today={today} onChange={(date) => set({ date })} />
        </div>
        <label>
          {t('debts.note')}
          <input name="note" maxLength={PAY_NOTE_MAX} autoComplete="off" placeholder={t('common.optional')} value={fields.note} onChange={(event) => set({ note: event.target.value })} />
        </label>
      </form>
    </Modal>
  )
}
