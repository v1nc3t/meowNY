import { useState, type FormEvent } from 'react'
import type { Category } from '../categoriesView'
import { categories, cleanCategoryName, type CategoryType } from '../api/categories'
import { errorText } from '../api/invoke'
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
    const name = cleanCategoryName(String(new FormData(event.currentTarget).get('name') ?? ''))
    if (!name) {
      setMessage('enter a category name of up to 50 characters.')
      return
    }
    setPending(true)
    try {
      onCreated(await categories.create(name, type as CategoryType))
    } catch (error) {
      setMessage(errorText(error))
    } finally {
      setPending(false)
    }
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
