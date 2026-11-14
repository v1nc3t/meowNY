import type { Account } from '../api/accounts'
import type { Category, CategoryType } from '../api/categories'
import type { NewTransaction, Transaction } from '../api/transactions'
import type { MessageKey } from '../i18n/en'
import { transferStart } from './accountDialogs'
import { activeAccounts, defaultAccountId, several } from './accountsView'
import { amountString } from './money'
import { draftOfBill, isShared, NOT_SPLIT, type SplitState } from './split'

// What the add dialog decides, apart from drawing: its tabs, what a change of tab keeps, the categories and the account of each tab, and the
// check of a transaction before it is sent (docs/accounts-design.md, sections 20.1 and 20.7).

/** An entry is an expense, an income or a transfer (Firefly's withdrawal, deposit and transfer). */
export type EntryTab = 'EXPENSE' | 'INCOME' | 'TRANSFER'

/** The tabs in their order: `Alt+1`, `Alt+2` and `Alt+3` choose them. */
export const ENTRY_TABS: readonly EntryTab[] = ['EXPENSE', 'INCOME', 'TRANSFER']

export const TAB_LABEL: Record<EntryTab, MessageKey> = { EXPENSE: 'common.expense', INCOME: 'common.income', TRANSFER: 'entry.tabTransfer' }

/**
 * The tabs a dialog has. None when a row is being changed: it keeps its kind (to change it, delete it and add another). Expense and income
 * when adding, and transfer too where the dialog can make one (`transfer`) and there are two accounts that are not archived to move money between.
 */
export function entryTabs(accounts: readonly Account[] | null, options: { editing: boolean; transfer: boolean }): EntryTab[] {
  if (options.editing) return []
  const moves = options.transfer && accounts !== null && activeAccounts(accounts).length > 1
  return moves ? [...ENTRY_TABS] : ['EXPENSE', 'INCOME']
}

/**
 * The tab an `Alt` + digit key press chooses: 1 is the first tab, 2 the second, 3 the third. The key's code is used, not what it types (a Mac's
 * `Option+1` types another character). Null for another key, one with more modifiers, or a tab this dialog has not.
 */
export function tabForKey(tabs: readonly EntryTab[], event: { code: string; altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }): EntryTab | null {
  if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return null
  const digit = /^Digit([1-9])$/.exec(event.code)
  return digit ? (tabs[Number(digit[1]) - 1] ?? null) : null
}

/** What a dialog has to know to start fresh and to change tab. */
export type EntryContext = {
  /** Every account, archived ones too. */
  accounts: readonly Account[]
  tabs: readonly EntryTab[]
  today: string
  /** The account the opener asked for. An archived or unknown one counts as none: the account a new row starts with is taken. */
  accountId?: string
}

/** The fields of the dialog, whatever the tab: the others are simply not shown or sent. */
export type EntryState = {
  tab: EntryTab
  name: string
  /** As typed, "12,5" too. */
  amount: string
  date: string
  /** The description of an expense or an income, the note of a transfer. */
  text: string
  categoryId: string
  /** The account of an expense or an income; '' when there is none. */
  account: string
  from: string
  to: string
  split: SplitState
}

/** A new entry on `tab` (the first tab when this dialog has not that one): empty, with the date of today and the accounts a new row starts with. */
export function entryStart(context: EntryContext, tab: EntryTab = 'EXPENSE'): EntryState {
  const sides = transferStart(context.accounts, context.accountId)
  return {
    tab: context.tabs.includes(tab) ? tab : 'EXPENSE',
    name: '',
    amount: '',
    date: context.today,
    text: '',
    categoryId: '',
    account: sides.from,
    ...sides,
    split: NOT_SPLIT,
  }
}

/** The tab of a row by its category's type: an income for an income category, an expense for the rest (also a category that is gone, as the forms always took it). */
export function tabOfCategory(categories: readonly Category[], categoryId: string): 'EXPENSE' | 'INCOME' {
  return categories.find((category) => category.id === categoryId)?.type === 'INCOME' ? 'INCOME' : 'EXPENSE'
}

/** A transaction being changed: its fields, on the tab of its category's type (an expense when the category is gone, as the form always took it). */
export function entryOf(transaction: Transaction, categories: readonly Category[]): EntryState {
  return {
    tab: tabOfCategory(categories, transaction.categoryId),
    name: transaction.name,
    amount: transaction.amount,
    date: transaction.paymentDate,
    text: transaction.description ?? '',
    categoryId: transaction.categoryId,
    account: transaction.accountId,
    from: '',
    to: '',
    split: isShared(transaction) ? { on: true, draft: draftOfBill(transaction.split, transaction.paidBy), ready: true, touched: false } : NOT_SPLIT,
  }
}

/**
 * Changing tab. What both tabs have stays: the amount (as typed), the date and the text; between an expense and an income also the name and
 * the account. The rest starts again: the category, the bill split with friends, and the accounts of a transfer. The same tab, a tab the
 * dialog has not, and any change in a dialog with no tabs (a row being changed) give back the same `state`.
 */
export function changeTab(state: EntryState, tab: EntryTab, context: EntryContext): EntryState {
  if (tab === state.tab || !context.tabs.includes(tab)) return state
  const kept = state.tab !== 'TRANSFER' && tab !== 'TRANSFER' ? { name: state.name, account: state.account } : {}
  return { ...entryStart(context, tab), amount: state.amount, date: state.date, text: state.text, ...kept }
}

/**
 * The categories a tab lists: those of its type. A transaction being changed whose category is gone still has it, in the first place, under
 * `deleted.label`.
 */
export function categoryChoices(categories: readonly Category[], type: CategoryType, deleted?: { id: string; label: string }): { value: string; label: string }[] {
  const options = categories.filter((category) => category.type === type).map((category) => ({ value: category.id, label: category.name }))
  return deleted && categories.every((category) => category.id !== deleted.id) ? [{ value: deleted.id, label: deleted.label }, ...options] : options
}

/**
 * The account field of an expense or an income. `shown`: an expense has it only with more than one account (the rule that hides it all), an
 * income always. `chosen` is what was picked (null: nothing yet), `current` the account the row already has. `value`: what the field holds
 * ('' when there is no account at all). `send`: the account to send, or undefined when there is nothing to say: no field, or the account the row
 * already has. `missing`: an income with no account, which cannot be saved.
 */
export function accountField(
  accounts: readonly Account[] | null,
  tab: 'EXPENSE' | 'INCOME',
  chosen: string | null,
  current?: string,
): { shown: boolean; value: string; send: string | undefined; missing: boolean } {
  const shown = tab === 'INCOME' || several(accounts)
  const value = chosen ?? current ?? (accounts ? defaultAccountId(accounts) : '')
  return { shown, value, send: shown && value !== '' && value !== current ? value : undefined, missing: tab === 'INCOME' && value === '' }
}

export type TransactionBody = Omit<NewTransaction, 'clientUuid'>

/**
 * The transaction an expense or an income tab holds, or why it cannot be sent. `remember` is the account to start the next new row in ('' when
 * the form has no account field). An edit says nothing of the account it keeps (it may be an archived one).
 */
export function transactionBody(entry: EntryState, accounts: readonly Account[], current: Transaction | null): { body: TransactionBody; remember: string } | { error: MessageKey } {
  const name = entry.name.trim()
  const categoryId = entry.categoryId
  if (!name || !entry.date || !categoryId) return { error: 'tx.errorRequired' }
  const amount = amountString(entry.amount, false)
  if (amount === null) return { error: 'tx.errorAmount' }
  const account = accountField(accounts, entry.tab === 'INCOME' ? 'INCOME' : 'EXPENSE', entry.account, current?.accountId)
  if (account.missing) return { error: 'entry.errorAccount' }
  const description = entry.text.trim()
  return {
    body: { name, amount, paymentDate: entry.date, categoryId, description: description === '' ? null : description, ...(account.send ? { accountId: account.send } : {}) },
    remember: account.shown ? account.value : '',
  }
}
