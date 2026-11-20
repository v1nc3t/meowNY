import { useId, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import type { Account } from '../api/accounts'
import { errorText } from '../api/invoke'
import { stashes as stashesApi, type Detail, type Entry, type EntryKind, type Stash } from '../api/stashes'
import { useToast } from '../hooks/toast'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { dialogNext, type DialogState } from '../lib/accountDialogs'
import { accountById, accountLabel, accountName } from '../lib/accountsView'
import { amountString, fromMinor, moneyIn } from '../lib/money'
import {
  canAdd,
  canTake,
  checkEntry,
  CHIP_WORDS,
  chips,
  deltaCheck,
  ENTRY_NOTE_MAX,
  figures,
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
  savedAfter,
  signedAmount,
  type MoneyFields,
} from '../lib/stashView'
import { tabId } from '../lib/tabs'
import ConfirmModal from './ConfirmModal'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import Modal, { modalFormId } from './Modal'
import SectionTabs from './SectionTabs'
import Button from './ui/Button'
import Icon from './NavIcon'
import { cn } from './ui/cn'

const deleteFormId = 'stash-entry-delete-form'

/**
 * Money into a stash or out of it, or an entry being changed (docs/stash-design.md, section 13.4). Add and take out are two tabs (`Alt+1`, `Alt+2`); the
 * amount, the date (never after today) and a note are kept when the tab changes. Under the amount are one-tap chips (the rest to the target, a month's
 * share, all that is free; all that is saved, half of it), and under them a bar that shows the stash before (solid) and after (striped: green for
 * money in, red for money out) with what it would then hold. When the amount breaks the rule of section 3.1 the line says why, in red, and the
 * button is off. An add can bring its money from another account, a take can send it on to one: a transfer is made with it. An entry being changed
 * has its amount, date and note, says what its transfer was and that it is not changed, and can be deleted (with its transfer, when it has one and
 * the person says so). The core checks everything again; the states are those of section 5.3.
 */
export default function StashMoneyModal({
  stash,
  tab,
  entry = null,
  accounts,
  settings,
  today,
  onClose,
  onSaved,
}: {
  /** The stash as the page has it: its numbers are what the dialog checks against. */
  stash: Stash
  /** The tab a new entry opens on. */
  tab: EntryKind
  /** The entry to change; none for a new one. */
  entry?: Entry | null
  /** Every account, archived ones too: the other side of a transfer is chosen among them. */
  accounts: Account[]
  settings: MoneySettings
  today: string
  onClose: () => void
  /** Told when an entry was added, changed or deleted, with the stash as it is now, even if the dialog was closed meanwhile. */
  onSaved: (detail: Detail) => void
}) {
  const t = useT()
  const say = useToast()
  const [fields, setFields] = useState<MoneyFields>(() => (entry ? moneyOfEntry(entry) : moneyStart(tab, today)))
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
  })
  const change = typed === null ? 0n : signedAmount(kind, typed) - (entry ? signedAmount(kind, entry.amount) : 0n)
  const after = savedAfter(stash.saved, change)
  const bar = previewBar(stash.saved, after, stash.target)
  const ownAccount = accountById(accounts, stash.accountId)
  const ownName = ownAccount ? accountName(ownAccount, t) : stash.account
  const otherName = (id: string) => {
    const account = accountById(accounts, id)
    return account ? accountName(account, t) : ''
  }

  /** What the line under the bar says: why the amount is not allowed, what the stash would hold, or (nothing typed yet) what it holds now. */
  function line(): { text: string; broken: boolean } {
    if (typed !== null && !rule.ok) {
      if (rule.reason === 'overFree') return { text: t('stashes.overFree', { free: money(rule.free), account: ownName }), broken: true }
      if (rule.reason === 'overTarget') return { text: t('stashes.overTarget', { over: money(rule.over) }), broken: true }
      if (rule.reason === 'overSaved') return { text: t('stashes.overSaved', { saved: money(rule.saved) }), broken: true }
      return { text: t(stash.archived ? 'stashes.archivedHint' : 'stashes.accountArchivedHint'), broken: true }
    }
    // nothing typed yet: what the stash holds now
    if (typed === null) return { text: figures(stash, money, t), broken: false }
    const percent = percentOf(after, stash.target)
    return {
      text: stash.target === null ? t('stashes.afterSaved', { saved: money(after) }) : t('stashes.after', { saved: money(after), target: money(stash.target), percent: percent ?? 0 }),
      broken: false,
    }
  }
  const shownLine = line()

  const kinds: EntryKind[] = [...(canAdd(stash) ? (['ADD'] as const) : []), ...(canTake(stash) || kind === 'TAKE' ? (['TAKE'] as const) : [])]
  const title = entry ? t('stashes.editEntry') : kind === 'ADD' ? t('stashes.addTitleMoney', { name: stash.name }) : t('stashes.takeTitle', { name: stash.name })

  function chooseTab(next: EntryKind) {
    if (next === kind) return
    setFields((current) => moneyTab(current, next))
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
    if (!entry && typed !== null) say(t(kind === 'ADD' ? 'stashes.addedToast' : 'stashes.takenToast', { amount: money(typed), name: stash.name }), 'success')
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
  // deleting an entry takes back what it did: an add whose money was taken out since cannot go (the stash would be below 0), a take only within what is free
  const removal = entry ? deltaCheck(stash.saved, fromMinor(-signedAmount(entry.kind, entry.amount)), stash.free, stash.target) : null
  const removalText =
    removal === null || removal.ok
      ? ''
      : removal.reason === 'overFree'
        ? t('stashes.overFree', { free: money(removal.free), account: ownName })
        : removal.reason === 'overTarget'
          ? t('stashes.overTarget', { over: money(removal.over) })
          : removal.reason === 'overSaved'
            ? t('stashes.overSaved', { saved: money(removal.saved) })
            : ''
  const suggestions = entry ? [] : chips(stash, kind)
  const code = stash.currency.toUpperCase() !== settings.currency.toUpperCase() ? stash.currency.toUpperCase() : null
  const transferFrom = kind === 'ADD'
  const otherId = fields.account
  const transferWords =
    bring && typed !== null
      ? t('stashes.transferLine', {
          amount: money(typed),
          from: transferFrom ? otherName(otherId) : ownName,
          to: transferFrom ? ownName : otherName(otherId),
        })
      : ''

  return (
    <>
      <Modal
        title={title}
        pending={state === 'SAVING' || blocked}
        onClose={onClose}
        leading={
          entry ? (
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
                value={fields.amount}
                onChange={(event) => set({ amount: event.target.value })}
              />
            </label>
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
              <p id={previewId} className={cn('stash-after', shownLine.broken && 'is-broken')} role="status">
                {shownLine.text}
              </p>
            </div>
            <div className="tx-date-group">
              <span>{t('report.date')}</span>
              <DatePicker kind="date" value={fields.date} label={t('report.date')} today={today} onChange={(date) => set({ date: notAfter(date, today) })} />
            </div>
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
                    {transferWords ? <p className="hint stash-transfer-line">{transferWords}</p> : null}
                  </>
                ) : null}
              </>
            ) : null}
            {entry && entry.transferId !== null && entry.otherAccount ? (
              <div className="stash-linked">
                <p className="hint">{t(entry.kind === 'ADD' ? 'stashes.cameWith' : 'stashes.wentWith', { account: entry.otherAccount })}</p>
                <p className="hint">{t('stashes.transferKept')}</p>
              </div>
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
