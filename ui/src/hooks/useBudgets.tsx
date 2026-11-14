import { useEffect, useState, type ComponentProps, type ReactNode } from 'react'
import { budgets as budgetsApi, MONTH, type Overview, type Span } from '../api/budgets'
import { categories as categoriesApi, type Category, type CategoryInMonth, type CategoryType } from '../api/categories'
import { errorText } from '../api/invoke'
import { monthLabel } from '../api/months'
import AddCategoryModal from '../components/AddCategoryModal'
import type BudgetHalf from '../components/BudgetHalf'
import BudgetSpanModal from '../components/BudgetSpanModal'
import EditCategoryModal from '../components/EditCategoryModal'
import { amountsFor, type Side } from '../lib/categoriesView'
import { amountString, formatMoney } from '../lib/money'
import { applyOrder } from '../lib/reorder'
import { useI18n } from '../i18n/context'
import { useToast } from './toast'
import type { MoneySettings } from './useMoneySettings'

/**
 * Everything one side of the budgets needs: its categories and its overview for a month, setting the budget, reordering, and the
 * dialogs to add, change and delete a category (`dialogs`, drawn anywhere on the page). `half` is what `BudgetHalf` takes; `reload`
 * changes whenever something was saved, for the parts of the page that read from the same data (the history).
 */
export function useBudgets({ type, month, thisMonth, settings }: { type: Side; month: string; thisMonth: string; settings: MoneySettings }): {
  half: Omit<ComponentProps<typeof BudgetHalf>, 'hideTitle'>
  dialogs: ReactNode
  add: () => void
  reload: number
  /** An amount for a list: zero and "not set" both read as a dash. */
  money: (amount: string) => string
} {
  const { t, language } = useI18n()
  const say = useToast()
  const [overview, setOverview] = useState<Overview[] | null>(null)
  const [categories, setCategories] = useState<CategoryInMonth[] | null>(null)
  const [reload, setReload] = useState(0)
  const refresh = () => setReload((value) => value + 1)
  // the dialogs: a new category, the one being changed, the one about to be deleted
  const [adding, setAdding] = useState(false)
  const [editing, setEditing] = useState<Category | null>(null)
  const [deleting, setDeleting] = useState<Category | null>(null)
  const [pending, setPending] = useState(false)
  // The answer to "how far should the change reach", while that question is open (null: cancelled).
  const [spanAnswer, setSpanAnswer] = useState<((span: Span | null) => void) | null>(null)

  useEffect(() => {
    let cancelled = false
    const loading = MONTH.test(month)
      ? categoriesApi.listInMonth(month)
      : categoriesApi.list().then((list) => list.map((category) => ({ ...category, deleted: false })))
    void loading.then(
      (list) => {
        if (!cancelled) setCategories(list)
      },
      (failure) => {
        if (cancelled) return
        say(errorText(failure), 'error')
        setCategories([])
      },
    )
    return () => {
      cancelled = true
    }
  }, [month, reload, say])

  useEffect(() => {
    if (!MONTH.test(month)) return
    let cancelled = false
    void budgetsApi.overview(month).then(
      (result) => {
        if (!cancelled) setOverview(result)
      },
      (failure) => {
        if (cancelled) return
        say(errorText(failure), 'error')
        setOverview([])
      },
    )
    return () => {
      cancelled = true
    }
  }, [month, reload, say])

  async function onDelete(span: Span) {
    if (!deleting || pending) return
    setPending(true)
    try {
      await categoriesApi.remove(deleting.id, month, span)
    } catch (failure) {
      say(errorText(failure), 'error')
    } finally {
      setPending(false)
    }
    setDeleting(null)
    refresh()
  }

  async function save(body: { type: string; scope: 'GLOBAL' | 'CATEGORY'; amountLimit: string | null; categoryId?: string }, span?: Span): Promise<string | null> {
    if (!MONTH.test(month)) return t('budgets.chooseMonth')
    try {
      await budgetsApi.set({ ...body, type: body.type as Side, month }, span)
    } catch (failure) {
      return errorText(failure)
    }
    refresh()
    return null
  }

  function amountFrom(raw: string): string | null | undefined {
    const trimmed = raw.trim()
    if (trimmed === '') return null
    const amount = amountString(trimmed, true)
    if (!amount) return undefined
    return amount
  }

  async function onGlobal(side: Side, raw: string): Promise<string | null> {
    const amountLimit = amountFrom(raw)
    if (amountLimit === undefined) {
      say(t('budgets.amountError'), 'error')
      return t('budgets.amountError')
    }
    // In a past month the person says how far the change reaches; cancelling leaves the budget as it was.
    let span: Span | undefined
    const current = overview?.find((row) => row.type === side)?.budgeted ?? null
    if (month < thisMonth && amountLimit !== current) {
      const answer = await new Promise<Span | null>((resolve) => setSpanAnswer(() => resolve))
      if (answer === null) return null
      span = answer
    }
    const error = await save({ type: side, scope: 'GLOBAL', amountLimit }, span)
    if (error) say(error, 'error')
    return error
  }

  // The new order shows at once; if it cannot be saved, the saved one comes back.
  async function onReorder(ids: string[]) {
    setCategories((current) => current && applyOrder(current, ids))
    try {
      await categoriesApi.reorder(ids)
    } catch (failure) {
      say(errorText(failure), 'error')
      refresh()
    }
  }

  const money = (amount: string) => (Number(amount) === 0 ? '-' : formatMoney(amount, settings.currency, settings.locale))

  return {
    half: {
      type,
      month,
      overview: MONTH.test(month) ? overview : [],
      categories: categories?.filter((category) => category.type === type) ?? null,
      money,
      onGlobal,
      onEdit: setEditing,
      onDelete: setDeleting,
      onReorder: (ids) => void onReorder(ids),
      onAdd: () => setAdding(true),
    },
    dialogs: (
      <>
        {adding ? (
          <AddCategoryModal
            type={type as CategoryType}
            month={month}
            onClose={() => setAdding(false)}
            onCreated={() => {
              setAdding(false)
              refresh()
            }}
          />
        ) : null}
        {editing ? (
          <EditCategoryModal
            key={`${editing.id}:${month}`}
            category={editing}
            amount={amountsFor(overview, editing.type).get(editing.id) ?? ''}
            month={month}
            monthText={monthLabel(month, language)}
            onClose={() => setEditing(null)}
            onSaved={() => {
              setEditing(null)
              refresh()
            }}
          />
        ) : null}
        {spanAnswer ? (
          <BudgetSpanModal
            onChoose={(span) => {
              spanAnswer(span)
              setSpanAnswer(null)
            }}
            onClose={() => {
              spanAnswer(null)
              setSpanAnswer(null)
            }}
          />
        ) : null}
        {deleting ? (
          <BudgetSpanModal
            title={t('budgets.deleteTitle', { name: deleting.name })}
            labels={{ ONLY: t('budgets.deleteOnly'), FORWARD: t('budgets.deleteForward'), ALL: t('budgets.deleteAll') }}
            hints={{ ONLY: t('budgets.deleteOnlyHint'), FORWARD: t('budgets.deleteForwardHint'), ALL: t('budgets.deleteAllHint') }}
            danger
            pending={pending}
            onChoose={(span) => void onDelete(span)}
            onClose={() => setDeleting(null)}
          />
        ) : null}
      </>
    ),
    add: () => setAdding(true),
    reload,
    money,
  }
}
