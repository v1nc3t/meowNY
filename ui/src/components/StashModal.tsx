import { useId, useMemo, useState, type FormEvent } from 'react'
import type { Account } from '../api/accounts'
import { ApiError, errorText } from '../api/invoke'
import { stashes as stashesApi, type Stash } from '../api/stashes'
import { useToast } from '../hooks/toast'
import { useI18n } from '../i18n/context'
import { NAME_MAX } from '../api/categories'
import { dialogNext, type DialogState } from '../lib/accountDialogs'
import { accountById, accountLabel, accountName } from '../lib/accountsView'
import { aMonthOn, stashBlock, stashFields, stashSubmit, type StashFields } from '../lib/stashDialogs'
import { homeAccounts, STASH_NOTES_MAX } from '../lib/stashView'
import { moneyIn, formatMoney } from '../lib/money'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import Icon from './NavIcon'
import Modal, { modalFormId } from './Modal'
import Button from './ui/Button'

/**
 * A new stash, or one being changed: its name, the account it lives in (always asked, even with one account; a stash that is made already only
 * moves to an account in its own currency), an optional target and target date, and notes. An edit sends only what changed, and one that changed
 * nothing just closes. The states are those of docs/stash-design.md, section 5.3: a wrong field is a toast and the fields stay as typed; a name
 * that is taken, or a refusal of the core, is said in place; what the core would refuse for the numbers (a target below what is saved, a move to an
 * account without room for what the stash holds) is said before and stops the button.
 */
export default function StashModal({
  stash,
  accounts,
  accountId,
  today,
  onClose,
  onSaved,
}: {
  /** The stash to change; none for a new one. */
  stash: Stash | null
  /** Every account, archived ones too. */
  accounts: Account[]
  /** The account a new stash starts in: the one of the page it was opened from. An archived or unknown one is not used. */
  accountId?: string
  today: string
  onClose: () => void
  /** Told when the stash is saved, even if the dialog was closed while saving, so the page can read its list again. */
  onSaved: (saved: Stash) => void
}) {
  const { t, language } = useI18n()
  const say = useToast()
  const [start] = useState<StashFields>(() => stashFields(stash, accounts, accountId))
  const [fields, setFields] = useState(start)
  const [state, setState] = useState<DialogState>('EDITING')
  // what the core said about a field: shown in place until the field is changed
  const [nameTaken, setNameTaken] = useState(false)
  const [refused, setRefused] = useState(false)
  const set = (patch: Partial<StashFields>) => {
    setNameTaken(false)
    setRefused(false)
    setFields((current) => ({ ...current, ...patch }))
  }
  const hintId = useId()
  const errorId = useId()

  const home = useMemo(() => homeAccounts(accounts, stash), [accounts, stash])
  const chosen = accountById(accounts, fields.account)
  // the currency the amounts are in: the stash's, or the chosen account's for a new one
  const currency = stash?.currency ?? (chosen?.foreign ? chosen.currency : '')
  const block = stashBlock(stash, fields, accounts)
  const money = (amount: string) => (stash ? moneyIn(stash.currency, language)(amount) : formatMoney(amount, chosen?.currency ?? '', language))
  const blockText =
    block === null
      ? ''
      : block.reason === 'targetBelowSaved'
        ? t('stashes.targetBelowSaved', { saved: money(block.saved) })
        : t('stashes.moveLacks', { account: accountName(block.account, t), free: money(block.free), saved: money(block.saved) })

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = stashSubmit(state, stash, fields, block)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    if (step.do === 'close') return onClose()
    setState((current) => dialogNext(current, 'send'))
    let saved: Stash
    try {
      saved = 'create' in step.request ? await stashesApi.create(step.request.create) : await stashesApi.update(stash?.id ?? '', step.request.patch)
    } catch (failure) {
      // a name that is taken and a refusal are said where they belong; the rest is a toast
      if (failure instanceof ApiError && failure.code === 'name_exists') setNameTaken(true)
      else if (failure instanceof ApiError && failure.code === 'invalid_input') setRefused(true)
      else say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    onSaved(saved)
  }

  const label = (key: 'stashes.target') => (currency ? t('accounts.labelIn', { label: t(key), code: currency.toUpperCase() }) : t(key))
  return (
    <Modal title={stash ? t('stashes.editTitle') : t('stashes.addTitle')} pending={state === 'SAVING' || block !== null} onClose={onClose}>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          {t('common.name')}
          <input
            name="name"
            required
            maxLength={NAME_MAX}
            autoComplete="off"
            aria-invalid={nameTaken || undefined}
            aria-describedby={nameTaken ? errorId : undefined}
            value={fields.name}
            onChange={(event) => set({ name: event.target.value })}
          />
        </label>
        {nameTaken ? (
          <p className="field-error" id={errorId} role="alert">
            {t('error.name_exists')}
          </p>
        ) : null}
        <Dropdown
          label={t('stashes.account')}
          value={fields.account}
          options={home.choices.map((account) => ({ value: account.id, label: accountLabel(account, t) }))}
          onChange={(account) => set({ account })}
        />
        {home.hidden && stash ? <p className="hint">{t('stashes.sameCurrencyOnly', { currency: stash.currency.toUpperCase() })}</p> : null}
        <label>
          {label('stashes.target')}
          <input
            name="target"
            inputMode="decimal"
            autoComplete="off"
            placeholder={t('stashes.noTarget')}
            aria-describedby={block?.reason === 'targetBelowSaved' ? hintId : undefined}
            aria-invalid={block?.reason === 'targetBelowSaved' || undefined}
            value={fields.target}
            onChange={(event) => set({ target: event.target.value })}
          />
        </label>
        <div className="tx-date-group">
          <span>{t('stashes.targetDate')}</span>
          {fields.targetDate === '' ? (
            <Button variant="outline" className="stash-date-choose" onClick={() => set({ targetDate: aMonthOn(today) })}>
              {t('stashes.chooseDate')}
            </Button>
          ) : (
            <div className="stash-date">
              <DatePicker kind="date" value={fields.targetDate} label={t('stashes.targetDate')} today={today} onChange={(targetDate) => set({ targetDate })} />
              <Button variant="ghost" size="icon" className="icon-btn" aria-label={t('stashes.clearDate')} title={t('stashes.clearDate')} onClick={() => set({ targetDate: '' })}>
                <Icon name="x" className="btn-icon" />
              </Button>
            </div>
          )}
        </div>
        {block ? (
          <p className="field-error" id={hintId} role="alert">
            {blockText}
          </p>
        ) : null}
        <label>
          {t('stashes.notes')}
          <textarea name="notes" rows={3} maxLength={STASH_NOTES_MAX} placeholder={t('common.optional')} value={fields.notes} onChange={(event) => set({ notes: event.target.value })} />
        </label>
        {refused ? (
          <p className="field-error" role="alert">
            {t('error.invalid_input')}
          </p>
        ) : null}
      </form>
    </Modal>
  )
}
