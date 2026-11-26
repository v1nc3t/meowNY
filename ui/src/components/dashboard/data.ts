import { useEffect, useRef, useState } from 'react'
import { accounts as accountsApi } from '../../api/accounts'
import { debts } from '../../api/debts'
import { errorText } from '../../api/invoke'
import { recurring } from '../../api/recurring'
import { stashes as stashesApi } from '../../api/stashes'
import { stats, type YearPoint } from '../../api/stats'
import { transactions } from '../../api/transactions'
import type { CategoryType } from '../../api/categories'
import { useToast } from '../../hooks/toast'
import { monthsOf, previousRange, type Range } from '../../lib/dashboardPeriod'
import { fromMinor, toMinor } from '../../lib/money'
import { ratePercent, usedPercent } from '../../pages/dashboardMath'

// What the widgets read. The Rust side adds up whole months of everything (stats); a widget that is set to some accounts or categories is added up
// here from their transactions instead, in the same unit (what each cost the person, `myShare`).

/** Widgets that ask for the same thing at the same moment (the summary and the trend both want a year) share one answer. */
const recent = new Map<string, Promise<unknown>>()
function shared<T>(key: string, load: () => Promise<T>): Promise<T> {
  const hit = recent.get(key)
  if (hit) return hit as Promise<T>
  const promise = load()
  recent.set(key, promise)
  void promise.then(
    () => setTimeout(() => recent.delete(key), 1000),
    () => recent.delete(key),
  )
  return promise
}

export const loadWorth = () => shared('worth', () => accountsApi.summary())
export const loadOwed = () => shared('owed', () => debts.summary())
export const loadStashes = () => shared('stashes', async () => ({ summary: await stashesApi.summary(), list: await stashesApi.list() }))
export const loadDues = (days: number) => shared(`dues:${days}`, () => recurring.dues(days))

/** The data of a widget, null until it has come (and while `enabled` is false); it is read again when `key` (what it was asked for) changes. A failure is a toast. */
export function useData<T>(key: string, load: () => Promise<T>, enabled = true): T | null {
  const [state, setState] = useState<{ key: string; data: T } | null>(null)
  const latest = useRef(load)
  const say = useToast()
  useEffect(() => {
    latest.current = load
  })
  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    latest.current().then(
      (data) => {
        if (!cancelled) setState({ key, data })
      },
      (failure) => {
        if (!cancelled) say(errorText(failure), 'error')
      },
    )
    return () => {
      cancelled = true
    }
  }, [key, enabled, say])
  return state?.data ?? null
}

// -- income and expense by month ----------------------------------------------------------------------------------------------

export type Filter = { accountIds?: string[]; categoryIds?: string[] }

export const filterKey = (filter: Filter) => JSON.stringify([filter.accountIds ?? [], filter.categoryIds ?? []])

const PAGE = 100
/** No more than this many pages of transactions are read for one sum. */
const MAX_PAGES = 100

async function addUp(range: Range, accountId: string | undefined, categoryId: string | undefined, kind: CategoryType, into: Map<string, bigint>) {
  for (let page = 0; page < MAX_PAGES; page++) {
    const answer = await transactions.list({ from: range.from, to: range.to, accountId, categoryId, kind, limit: PAGE, offset: page * PAGE })
    for (const row of answer.transactions) {
      const month = row.paymentDate.slice(0, 7)
      into.set(month, (into.get(month) ?? 0n) + (toMinor(row.myShare) ?? 0n))
    }
    if ((page + 1) * PAGE >= answer.total) return
  }
}

function pointOf(month: string, income: bigint, expense: bigint): YearPoint {
  const savings = income - expense
  return { month, income: fromMinor(income), expense: fromMinor(expense), savings: fromMinor(savings), savingsRate: ratePercent(fromMinor(savings), fromMinor(income)) }
}

/** Income, expense and savings for each month of `range`: of everything, or of the accounts and categories in `filter`. */
export function loadPoints(range: Range, filter: Filter): Promise<YearPoint[]> {
  const months = monthsOf(range)
  if (!filter.accountIds?.length && !filter.categoryIds?.length) {
    const years = [...new Set(months.map((month) => month.slice(0, 4)))]
    return Promise.all(years.map((year) => shared(`year:${year}`, () => stats.yearTrend(year)))).then((lists) => {
      const byMonth = new Map(lists.flat().map((point) => [point.month, point]))
      return months.map((month) => byMonth.get(month) ?? pointOf(month, 0n, 0n))
    })
  }
  return shared(`points:${range.from}:${range.to}:${filterKey(filter)}`, async () => {
    const income = new Map<string, bigint>()
    const expense = new Map<string, bigint>()
    const jobs: Promise<void>[] = []
    for (const accountId of filter.accountIds?.length ? filter.accountIds : [undefined]) {
      for (const categoryId of filter.categoryIds?.length ? filter.categoryIds : [undefined]) {
        jobs.push(addUp(range, accountId, categoryId, 'INCOME', income), addUp(range, accountId, categoryId, 'EXPENSE', expense))
      }
    }
    await Promise.all(jobs)
    return months.map((month) => pointOf(month, income.get(month) ?? 0n, expense.get(month) ?? 0n))
  })
}

export type Totals = { income: string; expense: string; savings: string; savingsRate: number | null }

/** What a run of months comes to. */
export function totalsOf(points: readonly Pick<YearPoint, 'income' | 'expense'>[]): Totals {
  const sum = (pick: (point: Pick<YearPoint, 'income' | 'expense'>) => string) => points.reduce((total, point) => total + (toMinor(pick(point)) ?? 0n), 0n)
  const { income, expense, savings } = pointOf('', sum((point) => point.income), sum((point) => point.expense))
  return { income, expense, savings, savingsRate: ratePercent(savings, income) }
}

/** The totals of a range and of the one before it, for the change between them. */
export async function loadTotals(range: Range, filter: Filter): Promise<{ now: Totals; before: Totals }> {
  const [now, before] = await Promise.all([loadPoints(range, filter), loadPoints(previousRange(range), filter)])
  return { now: totalsOf(now), before: totalsOf(before) }
}

// -- budgets --------------------------------------------------------------------------------------------------------------------

export type BudgetRow = { key: string; name: string; type: CategoryType; budgeted: string | null; actual: string; pct: number | null; total: boolean }

/** No more than this many months are read for the budgets of a long range (the last ones). */
const MAX_BUDGET_MONTHS = 24

const plus = (a: bigint | null, b: string | null) => (b === null ? a : (a ?? 0n) + (toMinor(b) ?? 0n))

/** The budgets and what was spent against them over `range`: for each side its total and then each category; a long range adds its months up. */
export async function loadBudgetRows(range: Range): Promise<BudgetRow[]> {
  const months = monthsOf(range).slice(-MAX_BUDGET_MONTHS)
  const lists = await Promise.all(months.map((month) => shared(`bva:${month}`, () => stats.budgetVsActual(month))))
  const totals = new Map<CategoryType, { budgeted: bigint | null; actual: bigint }>()
  const lines = new Map<string, { name: string; type: CategoryType; budgeted: bigint | null; actual: bigint }>()
  for (const bva of lists) {
    for (const o of bva.totals) {
      const was = totals.get(o.type)
      totals.set(o.type, { budgeted: plus(was?.budgeted ?? null, o.budgeted), actual: (was?.actual ?? 0n) + (toMinor(o.actual) ?? 0n) })
    }
    for (const c of bva.categories) {
      const was = lines.get(c.categoryId)
      lines.set(c.categoryId, { name: c.name, type: c.type, budgeted: plus(was?.budgeted ?? null, c.budgeted), actual: (was?.actual ?? 0n) + (toMinor(c.actual) ?? 0n) })
    }
  }
  const row = (key: string, name: string, type: CategoryType, budgeted: bigint | null, actual: bigint, total: boolean): BudgetRow => {
    const budget = budgeted === null ? null : fromMinor(budgeted)
    return { key, name, type, budgeted: budget, actual: fromMinor(actual), pct: usedPercent(fromMinor(actual), budget), total }
  }
  return (['EXPENSE', 'INCOME'] as const).flatMap((type) => [
    ...[...totals].filter(([kind, o]) => kind === type && o.budgeted !== null).map(([, o]) => row(`total-${type}`, '', type, o.budgeted, o.actual, true)),
    ...[...lines].filter(([, c]) => c.type === type && (c.budgeted !== null || c.actual !== 0n)).map(([id, c]) => row(id, c.name, type, c.budgeted, c.actual, false)),
  ])
}
