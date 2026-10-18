import { useState, type FormEvent } from 'react'
import { debts, type Person } from '../api/debts'
import { errorText } from '../api/invoke'
import { cleanCategoryName } from '../api/categories'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import Modal from './Modal'

// Own form id, so this modal also works when opened on top of another modal.
const formId = 'friend-form'

/** A friend is a name and nothing else: this adds one, or renames the one it is given. */
export default function FriendNameModal({
  person,
  onClose,
  onSaved,
}: {
  person?: Person
  onClose: () => void
  onSaved: (person: Person) => void
}) {
  const t = useT()
  const [message, setMessage] = useNotice()
  const [pending, setPending] = useState(false)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    // the same rule as a category's name: trimmed, not empty, 50 characters at most, no control characters
    const name = cleanCategoryName(String(new FormData(event.currentTarget).get('name') ?? ''))
    if (!name) {
      setMessage(t('debts.errorName'))
      return
    }
    setPending(true)
    try {
      onSaved(person ? await debts.updatePerson(person.id, { name }) : await debts.createPerson(name))
    } catch (failure) {
      setMessage(errorText(failure))
    } finally {
      setPending(false)
    }
  }

  return (
    <Modal title={person ? t('debts.renameTitle') : t('debts.addFriendTitle')} pending={pending} formId={formId} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={formId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          {t('common.name')}
          <input name="name" required maxLength={50} defaultValue={person?.name ?? ''} autoComplete="off" />
        </label>
      </form>
    </Modal>
  )
}
