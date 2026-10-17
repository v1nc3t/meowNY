import { useRef, useState, type FormEvent } from 'react'
import { budgets as budgetsApi } from '../api/budgets'
import { categories as categoriesApi, cleanCategoryName, type Category } from '../api/categories'
import { errorText } from '../api/invoke'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { amountString } from '../lib/money'
import Modal, { modalFormId } from './Modal'

/**
 * Changing a category: its name, and its budget from the month on the page (a budget is dated, so changing it never rewrites the
 * months before). Leaving the budget empty removes it from this month on.
 */
export default function EditCategoryModal({
  category,
  amount,
  month,
  monthText,
  onClose,
  onSaved,
}: {
  category: Category
  /** The budget of this category in `month`, or an empty text when it has none. */
  amount: string
  month: string
  /** The month as the person reads it, for the label. */
  monthText: string
  onClose: () => void
  onSaved: () => void
}) {
  const t = useT()
  const [message, setMessage] = useNotice()
  const [pending, setPending] = useState(false)
  // What is already saved, so a second try after an error does not do the first half again.
  const saved = useRef({ name: category.name })

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    const form = new FormData(event.currentTarget)
    const name = cleanCategoryName(String(form.get('name') ?? ''))
    if (!name) return setMessage(t('categories.nameError'))
    const raw = String(form.get('amount') ?? '').trim()
    const limit = raw === '' ? null : amountString(raw, true)
    if (raw !== '' && limit === null) return setMessage(t('budgets.amountError'))
    setPending(true)
    try {
      if (name !== saved.current.name) {
        await categoriesApi.rename(category.id, name)
        saved.current.name = name
      }
      if ((limit ?? '') !== amount) {
        await budgetsApi.set({ type: category.type, scope: 'CATEGORY', month, amountLimit: limit, categoryId: category.id })
      }
    } catch (failure) {
      setMessage(errorText(failure))
      return
    } finally {
      setPending(false)
    }
    onSaved()
  }

  return (
    <Modal title={t('categories.editTitle')} pending={pending} onClose={onClose}>
      <p className="notice">{message}</p>
      <form id={modalFormId} onSubmit={(event) => void onSubmit(event)}>
        <label>
          {t('common.name')}
          <input name="name" required maxLength={50} defaultValue={category.name} autoComplete="off" />
        </label>
        <label>
          {t('categories.budgetFrom', { month: monthText })}
          <input name="amount" inputMode="decimal" placeholder="0.00" defaultValue={amount} autoComplete="off" />
        </label>
        <p className="hint">{t('categories.budgetHint')}</p>
      </form>
    </Modal>
  )
}
