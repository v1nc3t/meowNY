import { useState, type FormEvent } from 'react'
import { accounts as accountsApi, ACCOUNT_KINDS, type Account, type AccountKind } from '../api/accounts'
import { errorText } from '../api/invoke'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { accountFields, accountSubmit, dialogNext, type AccountFields, type DialogState } from '../lib/accountDialogs'
import { accountName } from '../lib/accountsView'
import DatePicker from './DatePicker'
import Dropdown from './Dropdown'
import Modal, { modalFormId } from './Modal'

/**
 * A new account, or one being changed: its name, type, an optional number, and what it held on a day (the opening balance, which may be
 * below 0 and is 0.00 when left empty). An edit sends only what changed, and one that changed nothing just closes. The states are those
 * of docs/accounts-design.md, section 5.4: a wrong field or a failed save is a toast, and the fields stay as typed.
 */
export default function AccountModal({
  account,
  today,
  onClose,
  onSaved,
}: {
  /** The account to change; none for a new one. */
  account: Account | null
  today: string
  onClose: () => void
  /** Told when the account is saved. It is called even if the dialog was closed while saving, so the page can read its list again. */
  onSaved: (saved: Account) => void
}) {
  const t = useT()
  const say = useToast()
  const [start] = useState<AccountFields>(() => accountFields(account, account ? accountName(account, t) : '', today))
  const [fields, setFields] = useState(start)
  const [state, setState] = useState<DialogState>('EDITING')
  const set = (patch: Partial<AccountFields>) => setFields((current) => ({ ...current, ...patch }))

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = accountSubmit(state, account, account ? start : null, fields)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    if (step.do === 'close') return onClose()
    setState((current) => dialogNext(current, 'send'))
    let saved: Account
    try {
      saved = 'create' in step.request ? await accountsApi.create(step.request.create) : await accountsApi.update(account?.id ?? '', step.request.patch)
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    onSaved(saved)
  }

  return (
    <Modal title={account ? t('accounts.editTitle') : t('accounts.addTitle')} pending={state === 'SAVING'} onClose={onClose}>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          {t('common.name')}
          <input name="name" required maxLength={50} autoComplete="off" value={fields.name} onChange={(event) => set({ name: event.target.value })} />
        </label>
        {account?.isMain ? <p className="hint">{t('accounts.mainHint')}</p> : null}
        <Dropdown
          label={t('common.type')}
          value={fields.kind}
          options={ACCOUNT_KINDS.map((kind) => ({ value: kind, label: t(`accounts.kind.${kind}`) }))}
          onChange={(kind) => set({ kind: kind as AccountKind })}
        />
        <label>
          {t('accounts.number')}
          <input
            name="number"
            maxLength={50}
            autoComplete="off"
            spellCheck={false}
            placeholder={t('common.optional')}
            value={fields.number}
            onChange={(event) => set({ number: event.target.value })}
          />
        </label>
        <p className="hint">{t('accounts.numberHint')}</p>
        <label>
          {t('accounts.openingBalance')}
          {/* no decimal keypad: on some phones it has no minus, and a credit card starts below 0 */}
          <input name="balance" autoComplete="off" placeholder="0.00" value={fields.balance} onChange={(event) => set({ balance: event.target.value })} />
        </label>
        <div className="tx-date-group">
          <span>{t('accounts.openingDate')}</span>
          <DatePicker kind="date" value={fields.date} label={t('accounts.openingDate')} today={today} onChange={(date) => set({ date })} />
        </div>
      </form>
    </Modal>
  )
}
