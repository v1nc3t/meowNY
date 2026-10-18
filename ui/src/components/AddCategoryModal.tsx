import { useRef, useState, type FormEvent } from 'react'
import type { Category } from '../lib/categoriesView'
import { budgets } from '../api/budgets'
import { categories, cleanCategoryName, type CategoryType } from '../api/categories'
import { errorText } from '../api/invoke'
import { monthLabel } from '../api/months'
import { useToday } from '../api/today'
import Dropdown from './Dropdown'
import Modal from './Modal'
import { useNotice } from '../hooks/useNotice'
import { useI18n } from '../i18n/context'
import { amountString } from '../lib/money'

// Own form id, so this modal also works when opened on top of another modal.
const formId = 'category-form'

/**
 * A new category: its name and type, and, if wanted, its budget from a month on (the month of the page it is made on, or this month).
 * The budget is dated like every budget, so it never changes the months before.
 */
export default function AddCategoryModal({
  month,
  onClose,
  onCreated,
}: {
  /** The month the budget starts in. This month when left out. */
  month?: string
  onClose: () => void
  onCreated: (category: Category) => void
}) {
  const { t, language } = useI18n()
  const today = useToday()
  const budgetMonth = month ?? today?.month ?? null
  // Made already, when the budget failed after it: a second try only sets the budget.
  const made = useRef<Category | null>(null)
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
    const raw = String(new FormData(event.currentTarget).get('budget') ?? '').trim()
    const limit = raw === '' ? null : amountString(raw, true)
    if (raw !== '' && limit === null) {
      setMessage(t('budgets.amountError'))
      return
    }
    setPending(true)
    try {
      const category = made.current ?? (await categories.create(name, type as CategoryType))
      made.current = category
      if (limit !== null && budgetMonth) {
        await budgets.set({ type: category.type, scope: 'CATEGORY', month: budgetMonth, amountLimit: limit, categoryId: category.id })
      }
      onCreated(category)
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
        {budgetMonth ? (
          <>
            <label>
              {t('categories.budgetFrom', { month: monthLabel(budgetMonth, language) })}
              <input name="budget" inputMode="decimal" placeholder="0.00" autoComplete="off" />
            </label>
            <p className="hint">{t('categories.budgetHint')}</p>
          </>
        ) : null}
      </form>
    </Modal>
  )
}
