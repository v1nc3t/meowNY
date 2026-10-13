import { useState, type FormEvent } from 'react'
import type { Category } from '../categoriesView'
import { api, errorMessage } from '../http'
import Dropdown from './Dropdown'
import Modal from './Modal'

const typeOptions = [
  { value: 'EXPENSE', label: 'expense' },
  { value: 'INCOME', label: 'income' },
]

// Own form id, so this modal also works when opened on top of another modal.
const formId = 'category-form'

export default function AddCategoryModal({
  onClose,
  onCreated,
}: {
  onClose: () => void
  onCreated: (category: Category) => void
}) {
  const [type, setType] = useState('EXPENSE')
  const [message, setMessage] = useState('')
  const [pending, setPending] = useState(false)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const name = String(new FormData(event.currentTarget).get('name') ?? '').trim()
    if (!name) {
      setMessage('enter a category name.')
      return
    }
    setPending(true)
    const response = await api('/api/v1/categories', {
      method: 'POST',
      body: JSON.stringify({ name, type }),
    })
    setPending(false)
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    onCreated((await response.json()) as Category)
  }

  return (
    <Modal title="add category" pending={pending} formId={formId} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={formId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          name
          <input name="name" required maxLength={50} autoComplete="off" />
        </label>
        <Dropdown label="type" value={type} options={typeOptions} onChange={setType} />
      </form>
    </Modal>
  )
}
