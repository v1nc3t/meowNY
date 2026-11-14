import { useState, type FormEvent } from 'react'
import type { Account } from '../api/accounts'
import { errorText } from '../api/invoke'
import { transfers as transfersApi, type Transfer } from '../api/transfers'
import { useToast } from '../hooks/toast'
import { useT } from '../i18n/context'
import { chooseSide, dialogNext, transferStart, transferSubmit, type DialogState, type TransferFields } from '../lib/accountDialogs'
import { accountOptions, rememberAccount } from '../lib/accountsView'
import Icon from './NavIcon'
import Modal, { modalFormId } from './Modal'
import TransferInputs from './TransferInputs'
import Button from './ui/Button'

/**
 * Money moved from one of the person's accounts to another: a new transfer, or one being changed. Choosing in one list the account the
 * other holds swaps the two, so the same account on both sides cannot be made. A side of an old transfer that is archived by now shows
 * as "name (archived)" and stays unless it is changed. The states are those of docs/accounts-design.md, section 5.4.
 */
export default function TransferModal({
  transfer,
  accounts,
  fromId,
  today,
  onClose,
  onSaved,
  onDelete,
}: {
  /** The transfer to change; none for a new one. */
  transfer: Transfer | null
  /** Every account, archived ones too. */
  accounts: Account[]
  /** The account a new transfer starts from: the one of the page it was asked on. */
  fromId?: string
  today: string
  onClose: () => void
  /** Told when the transfer is saved, even if the dialog was closed while saving. */
  onSaved: () => void
  /** Asked for the transfer being changed: the page asks "are you sure" and deletes it. */
  onDelete?: () => void
}) {
  const t = useT()
  const say = useToast()
  const [fields, setFields] = useState<TransferFields>(() =>
    transfer
      ? { from: transfer.fromAccountId, to: transfer.toAccountId, amount: transfer.amount, date: transfer.date, note: transfer.note ?? '' }
      : { ...transferStart(accounts, fromId), amount: '', date: today, note: '' },
  )
  const [state, setState] = useState<DialogState>('EDITING')
  const set = (patch: Partial<TransferFields>) => setFields((current) => ({ ...current, ...patch }))
  const choose = (side: 'from' | 'to', id: string) => setFields((current) => ({ ...current, ...chooseSide(current, side, id) }))
  // the accounts to choose from, and the sides the transfer was saved with if they are archived by now
  const options = accountOptions(accounts, t, [transfer?.fromAccountId, transfer?.toAccountId])

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const step = transferSubmit(state, transfer, fields)
    if (step.do === 'ignore') return
    if (step.do === 'refuse') return say(t(step.error), 'error')
    if (step.do === 'close') return onClose()
    setState((current) => dialogNext(current, 'send'))
    try {
      if ('create' in step.request) await transfersApi.create(step.request.create)
      else await transfersApi.update(transfer?.id ?? '', step.request.patch)
    } catch (failure) {
      say(errorText(failure), 'error')
      setState((current) => dialogNext(current, 'error'))
      return
    }
    setState((current) => dialogNext(current, 'ok'))
    // the next new row starts in the account the money came from
    rememberAccount(fields.from)
    onSaved()
  }

  return (
    <Modal title={transfer ? t('transfers.editTitle') : t('transfers.addTitle')} pending={state === 'SAVING'} onClose={onClose}>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <TransferInputs {...fields} options={options} today={today} onChoose={choose} onChange={set} />
        {transfer && onDelete ? (
          <Button variant="outline" size="sm" className="row-danger" onClick={onDelete}>
            <Icon name="trash" className="btn-icon" />
            {t('common.delete')}
          </Button>
        ) : null}
      </form>
    </Modal>
  )
}
