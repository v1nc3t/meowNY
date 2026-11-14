import type { MessageKey } from '../i18n/en'
import type { EntryTab } from './entryDialog'

// The tabs above the list of the transactions page, and the address that keeps the one shown (docs/accounts-design.md, section 20.7).

export type TxTab = 'all' | 'expenses' | 'income' | 'transfers'

export const TX_TAB_LABEL: Record<TxTab, MessageKey> = { all: 'common.all', expenses: 'budgets.expenses', income: 'budgets.income', transfers: 'tx.tabTransfers' }

/** The tabs the page has. Transfers only with more than one account: with one there is nothing to move money between. */
export function txTabs(multi: boolean): TxTab[] {
  return multi ? ['all', 'expenses', 'income', 'transfers'] : ['all', 'expenses', 'income']
}

/** The tab the address (`?tab=income`) names. No tab, a word nobody knows, and transfers where the page has none are all "all". */
export function txTabOf(search: string | URLSearchParams, multi: boolean): TxTab {
  const wanted = new URLSearchParams(search).get('tab')
  return txTabs(multi).find((tab) => tab === wanted && tab !== 'all') ?? 'all'
}

/** The address with this tab: "all" is no parameter, and whatever else the address holds stays. */
export function withTab(search: string | URLSearchParams, tab: TxTab): URLSearchParams {
  const params = new URLSearchParams(search)
  if (tab === 'all') params.delete('tab')
  else params.set('tab', tab)
  return params
}

/** The `kind` the list of transactions is asked for: only expenses, only income, or none (both) for "all". Transfers are another list. */
export function kindOf(tab: TxTab): 'EXPENSE' | 'INCOME' | undefined {
  return tab === 'expenses' ? 'EXPENSE' : tab === 'income' ? 'INCOME' : undefined
}

/** The tab of the add dialog that matches the list: the kind that is shown, an expense for "all". */
export function entryTabOf(tab: TxTab): EntryTab {
  return tab === 'income' ? 'INCOME' : tab === 'transfers' ? 'TRANSFER' : 'EXPENSE'
}

/**
 * Where the rows of the list start. The place belongs to a tab: another tab starts at the first row, also when the person comes back to
 * a tab with the browser's back button.
 */
export function offsetOf(at: { tab: TxTab; offset: number }, tab: TxTab): number {
  return at.tab === tab ? at.offset : 0
}
