import { useState, type FormEvent } from 'react'
import type { Category } from '../lib/categoriesView'
import { categories, cleanCategoryName, type CategoryType } from '../api/categories'
import { errorText } from '../api/invoke'
import Dropdown from './Dropdown'
import Modal from './Modal'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'

// Own form id, so this modal also works when opened on top of another modal.
const formId = 'category-form'

export default function AddCategoryModal({
  onClose,
  onCreated,
}: {
  onClose: () => void
  onCreated: (category: Category) => void
}) {
  const t = useT()
  const [type, setType] = useState('EXPENSE')
  const typeOptions = [
    { value: 'EXPENSE', label: t('common.expense') },
    { value: 'INCOME', label: t('common.income') },
  ]
  const [message, setMessage] = useNotice()
  const [pending, setPending] = useState(false)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const name = cleanCategoryName(String(new FormData(event.currentTarget).get('name') ?? ''))
    if (!name) {
      setMessage(t('categories.nameError'))
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
    <Modal title={t('categories.addTitle')} pending={pending} formId={formId} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={formId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          {t('common.name')}
          <input name="name" required maxLength={50} autoComplete="off" />
        </label>
        <Dropdown label={t('common.type')} value={type} options={typeOptions} onChange={setType} />
      </form>
    </Modal>
  )
}
