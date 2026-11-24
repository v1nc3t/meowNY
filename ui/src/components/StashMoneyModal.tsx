import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import type { Account } from '../api/accounts'
import { categories as categoriesApi, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { stashes as stashesApi, type Detail, type Entry, type EntryKind, type Stash } from '../api/stashes'
import { useToast } from '../hooks/toast'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { dialogNext, type DialogState } from '../lib/accountDialogs'
import { accountById, accountLabel, accountName } from '../lib/accountsView'
import { categoryChoices } from '../lib/entryDialog'
import { amountString, moneyIn } from '../lib/money'
import { saveChoice, storedId } from '../lib/sticky'
import {
  canAdd,
  canTake,
  checkEntry,
  checkRemoval,
  CHIP_WORDS,
  chips,
  ENTRY_NOTE_MAX,
  isBreak,
  kindForKey,
  moneyOfEntry,
  moneyStart,
  moneySubmit,
  moneyTab,
  notAfter,
  otherAccounts,
  percentOf,
  previewBar,
  reachedNow,
  returnDate,
  returnStart,
  returnStop,
  returnSubmit,
  savedAfter,
  signedAmount,
  type DeltaCheck,
  type MoneyFields,
  type MoneyMode,
  type ReturnFields,
} from '../lib/stashView'
import { tabId } from '../lib/tabs'
import ConfirmModal from './ConfirmModal'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import { FactStrip } from './Facts'
import Modal, { modalFormId } from './Modal'
import SectionTabs from './SectionTabs'
import Button from './ui/Button'
import Icon from './NavIcon'
import { cn } from './ui/cn'

const deleteFormId = 'stash-entry-delete-form'

/** Where the income category of the last return is kept for the next one. */
const LAST_RETURN_CATEGORY = 'stashes.returnCategory'

/**
 * Money into a stash or out of it, or an entry being changed (docs/stash-design.md, section 13.4). Add and take out are two tabs (`Alt+1`, `Alt+2`); the
 * amount, the date (never after today) and a note are kept when the tab changes. Under the amount are one-tap chips (the rest to the target, a month's
 * share, all that is free; all that is saved, half of it), and under them a bar that shows the stash before (solid) and after (striped: green for
 * money in, red for money out) with what it would then hold. When the amount breaks the rule of section 3.1 the line says why, in red, and the
 * button is off. An add can bring its money from another account, a take can send it on to one: a transfer is made with it. An entry being changed
 * has its amount, date and note, says what its transfer was and that it is not changed, and can be deleted (with its transfer, when it has one and
 * the person says so). The core checks everything again; the states are those of section 5.3.
 *
 * A deposit (docs/loans-deposits-design.md, sections 5.3 and 5.6) changes it in three ways. It has the add tab only while it runs. Taking out of one that
 * runs is a break: the amount is all that is saved and cannot be typed, a line says it ends the deposit, and the button says "break deposit". On a deposit
 * that is broken an entry keeps its amount and date and only the note changes, and it cannot be deleted; what the other states refuse (money in when it
 * does not run, money out of an entry while it runs) is said under the amount. The third mode, `RETURN`, is for a deposit that has matured: the amount
 * it expects to return, an income category (the last one used), a date and a note, with no transfer; the button books the return.
 */
type Props = {
  /** The stash as the page has it: its numbers are what the dialog checks against. */
  stash: Stash
  /** The mode a new entry opens in: an add, a take, or the return of a deposit that matured. */
  tab: MoneyMode
  /** The entry to change; none for a new one. */
  entry?: Entry | null
  /** Every account, archived ones too: the other side of a transfer is chosen among them. */
  accounts: Account[]
  settings: MoneySettings
  today: string
  onClose: () => void
  /** Told when an entry was added, changed or deleted, with the stash as it is now (null when a return was booked: the page reads it again), even if the dialog was closed meanwhile. */
  onSaved: (detail: Detail | null) => void
}

export default function StashMoneyModal(props: Props) {
  return props.tab === 'RETURN' && !props.entry ? <ReturnDialog {...props} /> : <MoneyForm {...props} tab={props.tab === 'RETURN' ? 'ADD' : props.tab} />
}

function MoneyForm({
  stash,
  tab,
  entry = null,
  accounts,
  settings,
  today,
  onClose,
  onSaved,
}: Props & { tab: EntryKind }) {
  const t = useT()
  const say = useToast()
  // taking out of a deposit that runs is its break: all that is saved, not typed
  const breaking = isBreak(stash)
  // a deposit that is broken keeps its entries: only a note changes
  const kept = entry !== null && stash.deposit?.state === 'BROKEN'
  const [fields, setFields] = useState<MoneyFields>(() => {
    if (entry) return moneyOfEntry(entry)
    const start = moneyStart(tab, today)
    return breaking && tab === 'TAKE' ? { ...start, amount: stash.saved } : start
  })
  const [state, setState] = useState<DialogState>('EDITING')
  const [confirming, setConfirming] = useState(false)
  const [alsoTransfer, setAlsoTransfer] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const amountRef = useRef<HTMLInputElement>(null)
  const previewId = useId()
  const set = (patch: Partial<MoneyFields>) => setFields((current) => ({ ...current, ...patch }))

  const money = moneyIn(stash.currency, settings.locale)
  const kind = fields.kind
  const typed = amountString(fields.amount, false)
  const others = entry ? [] : otherAccounts(accounts, stash)
  const bring = fields.transfer && fields.account !== ''
  // the dialog's numbers: what the amount does to the stash, whether the rule lets it, and what the stash would hold
  const rule = checkEntry({
    kind,
    saved: stash.saved,
    free: stash.free,
    target: stash.target,
    amount: typed ?? '0.00',
    old: entry?.amount,
    bring: bring && kind === 'ADD',
    closed: !canAdd(stash),
    deposit: stash.deposit?.state ?? null,
  })
  const change = typed === null ? 0n : signedAmount(kind, typed) - (entry ? signedAmount(kind, entry.amount) : 0n)
  const after = savedAfter(stash.saved, change)
  const bar = previewBar(stash.saved, after, stash.target)
  const ownAccount = accountById(accounts, stash.accountId)
  const ownName = ownAccount ? accountName(ownAccount, t) : stash.account

  /** Why a change is not allowed, in words. */
  function whyNot(why: Exclude<DeltaCheck, { ok: true }>): string {
    if (why.reason === 'overFree') return t('stashes.overFree', { free: money(why.free), account: ownName })
    if (why.reason === 'overTarget') return t('stashes.overTarget', { over: money(why.over) })
    if (why.reason === 'overSaved') return t('stashes.overSaved', { saved: money(why.saved) })
    if (why.reason === 'running') return t('stashes.runningNoOut')
    if (why.reason === 'broken') return t('stashes.brokenFixed')
    if (why.reason === 'notRunning') return t('stashes.notRunning')
    // closed: archived, its account archived, or a deposit that does not run
    return t(stash.archived ? 'stashes.archivedHint' : stash.accountArchived ? 'stashes.accountArchivedHint' : 'stashes.notRunning')
  }

  // under the bar: why the amount is not allowed, and the figures as a strip, now or (an amount typed) after the change
  const problem = typed !== null && !rule.ok ? whyNot(rule) : null
  const held = typed === null ? stash.saved : after

  const kinds: EntryKind[] = [...(canAdd(stash) ? (['ADD'] as const) : []), ...(canTake(stash) || kind === 'TAKE' ? (['TAKE'] as const) : [])]
  const title = entry ? t('stashes.editEntry') : kind === 'ADD' ? t('stashes.addTitleMoney', { name: stash.name }) : t('stashes.takeTitle', { name: stash.name })

  function chooseTab(next: EntryKind) {
    if (next === kind) return
    setFields((current) => moneyTab(current, next, breaking ? stash.saved : undefined))
  }

  // Alt+1 and Alt+2 choose the tab from anywhere in the dialog; the focus goes back to the amount, which is always there
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (entry) return
    const next = kindForKey(event)
    if (!next || !kinds.includes(next)) return
    event.preventDefault()
    chooseTab(next)
    amountRef.current?.focus()
  }

  function turnTransfer(on: boolean) {
    setFields((current) => ({ ...current, transfer: on, account: on ? (current.account || others[0]?.id || '') : '' }))
  }

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = moneySubmit(state, entry, fields, today, rule)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    if (step.do === 'close') return onClose()
    setState((current) => dialogNext(current, 'send'))
    let detail: Detail
    try {
      detail =
        'add' in step.request
          ? await stashesApi.add(stash.id, step.request.add)
          : 'take' in step.request
            ? await stashesApi.take(stash.id, step.request.take)
            : await stashesApi.amend(entry?.id ?? '', step.request.patch)
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    if (!entry && typed !== null) {
      say(t(kind === 'ADD' ? 'stashes.addedToast' : breaking ? 'stashes.brokenToast' : 'stashes.takenToast', { amount: money(typed), name: stash.name }), 'success')
    }
    if (reachedNow(stash, detail.stash)) say(t('stashes.reachedToast', { name: stash.name }), 'success')
    onSaved(detail)
  }

  async function onDelete() {
    if (!entry || deleting) return
    setDeleting(true)
    try {
      const detail = await stashesApi.erase(entry.id, alsoTransfer && entry.transferId !== null)
      setConfirming(false)
      onSaved(detail)
    } catch (failure) {
      say(errorText(failure), 'error')
      setDeleting(false)
    }
  }

  const blocked = typed === null || !rule.ok
  // the amount is not typed: all that is saved for a break, what the entry has for an entry that is kept
  const fixedAmount = kept || (breaking && kind === 'TAKE' && !entry)
  // deleting an entry takes back what it did: an add whose money was taken out since cannot go (the stash would be below 0), a take only within what is free
  const removal = entry ? checkRemoval(stash, entry) : null
  const removalText = removal === null || removal.ok ? '' : whyNot(removal)
  const suggestions = entry || (breaking && kind === 'TAKE') ? [] : chips(stash, kind)
  const code = stash.currency.toUpperCase() !== settings.currency.toUpperCase() ? stash.currency.toUpperCase() : null
  // what stays as it is is behind a help button beside the title, not lines in the form: a broken deposit keeps its entries, an entry keeps its transfer
  const help =
    kept || (entry && entry.transferId !== null && entry.otherAccount) ? (
      <>
        {kept ? <p>{t('stashes.brokenFixed')}</p> : null}
        {entry && entry.transferId !== null && entry.otherAccount ? (
          <>
            <p>{t(entry.kind === 'ADD' ? 'stashes.cameWith' : 'stashes.wentWith', { account: entry.otherAccount })}</p>
            <p>{t('stashes.transferKept')}</p>
          </>
        ) : null}
      </>
    ) : undefined

  return (
    <>
      <Modal
        title={title}
        pending={state === 'SAVING' || blocked}
        help={help}
        onClose={onClose}
        doneLabel={breaking && kind === 'TAKE' && !entry ? t('stashes.breakDeposit') : undefined}
        danger={breaking && kind === 'TAKE' && !entry}
        leading={
          entry && !kept ? (
            <Button variant="outline" className="row-danger" onClick={() => setConfirming(true)}>
              <Icon name="trash" className="btn-icon" />
              {t('common.delete')}
            </Button>
          ) : undefined
        }
      >
        <div className="stash-money" onKeyDown={onKeyDown}>
          {!entry && kinds.length > 1 ? (
            <SectionTabs
              label={t('stashes.moneyTabs')}
              value={kind}
              onChange={chooseTab}
              tabs={kinds.map((id) => ({ id, label: id === 'ADD' ? t('common.add') : t('stashes.takeOut') }))}
            />
          ) : null}
          <form id={modalFormId} className="stash-money-form" role={!entry && kinds.length > 1 ? 'tabpanel' : undefined} aria-labelledby={!entry && kinds.length > 1 ? tabId(kind) : undefined} onSubmit={(event) => void onSubmit(event)}>
            <label>
              {code ? t('entry.amountIn', { code }) : t('tx.amount')}
              <input
                ref={amountRef}
                name="amount"
                required
                inputMode="decimal"
                autoComplete="off"
                placeholder="0.00"
                data-autofocus
                aria-describedby={previewId}
                aria-invalid={(typed !== null && !rule.ok) || undefined}
                readOnly={fixedAmount}
                value={fields.amount}
                onChange={(event) => set({ amount: event.target.value })}
              />
            </label>
            {breaking && kind === 'TAKE' && !entry ? <p className="hint">{t('stashes.breakLine')}</p> : null}
            {suggestions.length > 0 ? (
              <div className="stash-chips">
                {suggestions.map((chip) => (
                  <button
                    key={chip.id}
                    type="button"
                    className="stash-chip"
                    aria-pressed={typed === chip.amount}
                    onClick={() => {
                      set({ amount: chip.amount })
                      amountRef.current?.focus()
                    }}
                  >
                    {t(CHIP_WORDS[chip.id], { amount: money(chip.amount) })}
                  </button>
                ))}
              </div>
            ) : null}
            <div className="stash-preview">
              {stash.target !== null || stash.saved !== '0.00' || typed !== null ? (
                <span className={cn('stash-track', bar.tone !== 'same' && `is-${bar.tone}`, typed !== null && !rule.ok && 'is-broken')} aria-hidden="true">
                  <span className="stash-solid" style={{ width: `${bar.solid * 100}%` }} />
                  <span className="stash-stripe" style={{ width: `${bar.stripe * 100}%` }} />
                </span>
              ) : null}
              <div id={previewId} role="status">
                <FactStrip
                  items={[
                    { label: t(typed === null ? 'stashes.savedLabel' : 'stashes.savedAfter'), value: money(held), tone: held.startsWith('-') ? 'negative' : undefined },
                    stash.target !== null && { label: t('stashes.target'), value: money(stash.target) },
                    stash.target !== null && { label: t('stashes.progress'), value: t('stashes.percent', { percent: percentOf(held, stash.target) ?? 0 }) },
                  ]}
                />
                <p className="stash-after is-broken">{problem}</p>
              </div>
            </div>
            {kept ? null : (
              <div className="tx-date-group">
                <span>{t('report.date')}</span>
                <DatePicker kind="date" value={fields.date} label={t('report.date')} today={today} onChange={(date) => set({ date: notAfter(date, today) })} />
              </div>
            )}
            <label>
              {t('debts.note')}
              <textarea
                name="note"
                rows={2}
                maxLength={ENTRY_NOTE_MAX}
                autoComplete="off"
                placeholder={t('common.optional')}
                value={fields.note}
                onChange={(event) => set({ note: event.target.value })}
                onKeyDown={(event) => {
                  // Enter sends the form, as in every other field; Shift+Enter makes a second line
                  if (event.key !== 'Enter' || event.shiftKey || event.altKey || event.ctrlKey || event.metaKey || event.nativeEvent.isComposing) return
                  event.preventDefault()
                  event.currentTarget.form?.requestSubmit()
                }}
              />
              <span className="stash-count" aria-hidden="true">
                {t('stashes.noteCount', { count: [...fields.note].length })}
              </span>
            </label>
            {others.length > 0 ? (
              <>
                <label className="check">
                  <input type="checkbox" checked={fields.transfer} onChange={(event) => turnTransfer(event.target.checked)} />
                  {t(kind === 'ADD' ? 'stashes.bringFrom' : 'stashes.sendTo')}
                </label>
                {fields.transfer ? (
                  <>
                    <Dropdown
                      label={t(kind === 'ADD' ? 'transfers.from' : 'transfers.to')}
                      value={fields.account}
                      options={others.map((account) => ({ value: account.id, label: accountLabel(account, t) }))}
                      onChange={(account) => set({ account })}
                    />
                  </>
                ) : null}
              </>
            ) : null}
          </form>
        </div>
      </Modal>
      {confirming && entry ? (
        <ConfirmModal
          title={t('rec.confirmTitle', { action: t('common.delete'), name: t('stashes.entryName', { amount: money(entry.amount) }) })}
          text={
            removalText ? (
              <span className="stash-after is-broken">{removalText}</span>
            ) : (
              t('stashes.deleteEntryText', { saved: money(savedAfter(stash.saved, -signedAmount(entry.kind, entry.amount))) })
            )
          }
          formId={deleteFormId}
          pending={deleting || removalText !== ''}
          extra={
            entry.transferId !== null ? (
              <label className="check stash-delete-transfer">
                <input type="checkbox" checked={alsoTransfer} onChange={(event) => setAlsoTransfer(event.target.checked)} />
                {t('stashes.deleteTransferToo')}
              </label>
            ) : null
          }
          onConfirm={() => void onDelete()}
          onClose={() => setConfirming(false)}
        />
      ) : null}
    </>
  )
}

/**
 * The return mode (section 5.6): the categories are read once, as in the payment dialog, and the form opens when they are there. A failed read closes
 * the dialog with a toast.
 */
function ReturnDialog({ stash, settings, today, onClose, onSaved }: Props) {
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
  return <ReturnForm stash={stash} categories={categories} settings={settings} today={today} onClose={onClose} onSaved={onSaved} />
}

/**
 * Books the return of a deposit that has matured: the amount it expects to return (typed over when the bank paid another), an income category (the
 * one of the last return while it is still one), the day (from the maturity to today) and a note. One request makes the income and the add in the core;
 * done is off while a field is wrong. The states are those of section 5.3: an error goes back to editing with a toast, a second submit while saving is ignored.
 */
function ReturnForm({
  stash,
  categories,
  settings,
  today,
  onClose,
  onSaved,
}: Pick<Props, 'stash' | 'settings' | 'today' | 'onClose' | 'onSaved'> & { categories: Category[] }) {
  const t = useT()
  const say = useToast()
  const [fields, setFields] = useState<ReturnFields>(() => returnStart(stash, categories, storedId(LAST_RETURN_CATEGORY), today))
  const [state, setState] = useState<DialogState>('EDITING')
  const set = (patch: Partial<ReturnFields>) => setFields((current) => ({ ...current, ...patch }))
  const money = moneyIn(stash.currency, settings.locale)
  const stop = returnStop(fields, stash.targetDate, today)
  const code = stash.currency.toUpperCase() !== settings.currency.toUpperCase() ? stash.currency.toUpperCase() : null
  const typed = amountString(fields.amount, false)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = returnSubmit(state, stash, fields, today)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    setState((current) => dialogNext(current, 'send'))
    try {
      await stashesApi.bookReturn(stash.id, step.request)
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    // the next return starts from the same category
    saveChoice(LAST_RETURN_CATEGORY, step.request.categoryId)
    say(t('stashes.returnedToast', { amount: money(step.request.amount), name: stash.name }), 'success')
    onSaved(null)
  }

  return (
    <Modal title={t('stashes.returnTitle', { name: stash.name })} doneLabel={t('stashes.bookReturn')} pending={state === 'SAVING' || stop !== null} onClose={onClose}>
      <form id={modalFormId} className="stash-money-form" onSubmit={(event) => void onSubmit(event)}>
        <label>
          {code ? t('entry.amountIn', { code }) : t('tx.amount')}
          <input
            name="amount"
            required
            inputMode="decimal"
            autoComplete="off"
            placeholder="0.00"
            data-autofocus
            aria-invalid={(fields.amount !== '' && typed === null) || undefined}
            value={fields.amount}
            onChange={(event) => set({ amount: event.target.value })}
          />
        </label>
        <Dropdown
          label={t('stashes.returnCategory')}
          value={fields.category}
          options={categoryChoices(categories, 'INCOME')}
          placeholder={t('tx.chooseCategory')}
          onChange={(category) => set({ category })}
        />
        <div className="tx-date-group">
          <span>{t('report.date')}</span>
          <DatePicker kind="date" value={fields.date} label={t('report.date')} today={today} onChange={(date) => set({ date: returnDate(date, stash.targetDate ?? '', today) })} />
        </div>
        <label>
          {t('debts.note')}
          <input name="note" maxLength={ENTRY_NOTE_MAX} autoComplete="off" placeholder={t('common.optional')} value={fields.note} onChange={(event) => set({ note: event.target.value })} />
        </label>
      </form>
    </Modal>
  )
}
