import type { Account } from '../api/accounts'
import type { Category, CategoryType } from '../api/categories'
import type { NewTransaction, Transaction } from '../api/transactions'
import type { MessageKey } from '../i18n/en'
import { transferStart } from './accountDialogs'
import { accountById, activeAccounts, defaultAccountId, mainCurrency, rateOf, several } from './accountsView'
import { amountString, invertAmount } from './money'
import { draftOfBill, isShared, NOT_SPLIT, type SplitState } from './split'

// What the add dialog decides, apart from drawing: its tabs, what a change of tab keeps, the categories and the account of each tab, and the
// check of a transaction before it is sent (docs/accounts-design.md, sections 20.1 and 20.7).

/** An entry is an expense, an income or a transfer. */
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
  /**
   * With an account in another currency chosen, `amount` is what it was in that currency, and this is what it was in the app's, as typed: the
   * amount that budgets and statistics count. It is only used once typed (`inMainTouched`); until then, and after it is emptied, the field shows what
   * the account's rate gives. Not used with an account in the app's currency.
   */
  inMain: string
  inMainTouched: boolean
  /** The same for a transfer: what reaches the second account, typed (`arrivesTouched`) or, until then, from the two rates. */
  arrives: string
  arrivesTouched: boolean
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
    inMain: '',
    inMainTouched: false,
    arrives: '',
    arrivesTouched: false,
  }
}

/** The tab of a row by its category's type: an income for an income category, an expense for the rest (also a category that is gone, as the forms always took it). */
export function tabOfCategory(categories: readonly Category[], categoryId: string): 'EXPENSE' | 'INCOME' {
  return categories.find((category) => category.id === categoryId)?.type === 'INCOME' ? 'INCOME' : 'EXPENSE'
}

/**
 * A transaction being changed: its fields, on the tab of its category's type (an expense when the category is gone, as the form always took it).
 * One in an account in another currency opens with both its amounts: what it was in that currency in the first field, and in the app's currency in
 * the second, which counts as typed (it is the stored one, and is not worked out again). With the `accounts`, a transaction whose account is in the
 * app's currency has one amount whatever else it says (a stale native amount means nothing there), and one in an account in another currency that
 * has no native amount (it should not happen) opens with that field empty, to be typed.
 */
export function entryOf(transaction: Transaction, categories: readonly Category[], accounts?: readonly Account[]): EntryState {
  const account = accounts ? accountById(accounts, transaction.accountId) : null
  const foreign = account ? account.foreign : typeof transaction.nativeAmount === 'string'
  return {
    tab: tabOfCategory(categories, transaction.categoryId),
    name: transaction.name,
    amount: foreign ? (transaction.nativeAmount ?? '') : transaction.amount,
    date: transaction.paymentDate,
    text: transaction.description ?? '',
    categoryId: transaction.categoryId,
    account: transaction.accountId,
    from: '',
    to: '',
    split: isShared(transaction) ? { on: true, draft: draftOfBill(transaction.split, transaction.paidBy), ready: true, touched: false } : NOT_SPLIT,
    inMain: foreign ? transaction.amount : '',
    inMainTouched: foreign,
    arrives: '',
    arrivesTouched: false,
  }
}

// -- an expense or an income in an account in another currency --------------------------------------------------------------------------

/** The account in effect for an expense or an income: what the field holds (the one picked, else the row's own, else the one a new row starts with). */
function accountInEffect(accounts: readonly Account[], entry: EntryState, current: Transaction | null): Account | null {
  if (entry.tab === 'TRANSFER') return null
  return accountById(accounts, accountField(accounts, entry.tab, entry.account, current?.accountId).value)
}

/**
 * What an expense or an income has when its account is in another currency than the app's: the code of that currency (the first field, `amount`,
 * is in it), the app's code, and what the second field holds. Null for an account in the app's currency, and for a transfer: one amount, as before.
 */
export function nativeView(accounts: readonly Account[], entry: EntryState, current: Transaction | null): { code: string; mainCode: string; inMain: string } | null {
  const account = accountInEffect(accounts, entry, current)
  if (!account?.foreign) return null
  return { code: account.currency.toUpperCase(), mainCode: mainCurrency(accounts).toUpperCase(), inMain: inMainText(accounts, entry, current) }
}

/** What the amount in the account's currency is worth in the app's at the account's rate, rounded half up to the cent; null when it is not an amount or there is no rate. */
export function suggestedInMain(accounts: readonly Account[], entry: EntryState, current: Transaction | null): string | null {
  const account = accountInEffect(accounts, entry, current)
  const rate = rateOf(account)
  const native = amountString(entry.amount, false)
  return account?.foreign && rate !== null && native !== null ? invertAmount(native, rate) : null
}

/** What the second field holds: what the person typed in it, and until then (and after they empty it) what the rate gives; empty when neither. */
export function inMainText(accounts: readonly Account[], entry: EntryState, current: Transaction | null): string {
  return entry.inMainTouched ? entry.inMain : (suggestedInMain(accounts, entry, current) ?? '')
}

/** The second field was typed in: it is theirs from now on; emptied, it follows the rate again. */
export function withInMain(entry: EntryState, text: string): EntryState {
  return { ...entry, inMain: text, inMainTouched: text.trim() !== '' }
}

/** The amount the bill is split over: the one in the app's currency (the app splits `amount`, as it counts it), so with an account in another currency the second field. */
export function splitAmount(accounts: readonly Account[], entry: EntryState, current: Transaction | null): string {
  return nativeView(accounts, entry, current)?.inMain ?? entry.amount
}

/**
 * The account of an expense or an income was changed. Between two accounts in the app's currency, or in the same one or another foreign one, only the
 * account changes: the first field keeps what was typed, and a second field that was not typed in follows the new rate (a typed one stays).
 *
 * To an account in another currency from one in the app's: in a new entry the number typed stays in the first field, which now says what currency it is
 * in, and the second field works out the app's amount from it. In a transaction being changed the amount it has is in the app's currency, so it goes to the
 * second field as typed there and the first one starts empty (a number of the app's currency is not an amount of the other one, and saving it so would
 * quietly change what budgets count).
 *
 * From an account in another currency to one in the app's: when the second field was typed in (a transaction being changed has it so), that amount is
 * the amount; when it was not, the number in the first field moves into the single amount field, where the person typed it.
 */
export function chooseAccount(accounts: readonly Account[], entry: EntryState, current: Transaction | null, id: string): EntryState {
  const before = accountInEffect(accounts, entry, current)?.foreign ?? false
  const after = accountById(accounts, id)?.foreign ?? false
  const next = { ...entry, account: id }
  if (before && !after) {
    return { ...next, amount: entry.inMainTouched ? entry.inMain : entry.amount, inMain: '', inMainTouched: false }
  }
  if (!before && after && current !== null) {
    return { ...next, amount: '', inMain: entry.amount, inMainTouched: entry.amount.trim() !== '' }
  }
  return next
}

/**
 * Changing tab. What both tabs have stays: the amount (as typed), the date and the text; between an expense and an income also the name, the account
 * and, with an account in another currency, the amount in the app's currency (typed or not). The rest starts again: the category, the bill split with
 * friends, the accounts of a transfer and its second amount. The same tab, a tab the dialog has not, and any change in a dialog with no tabs (a row being
 * changed) give back the same `state`.
 */
export function changeTab(state: EntryState, tab: EntryTab, context: EntryContext): EntryState {
  if (tab === state.tab || !context.tabs.includes(tab)) return state
  const between = state.tab !== 'TRANSFER' && tab !== 'TRANSFER'
  const kept = between ? { name: state.name, account: state.account, inMain: state.inMain, inMainTouched: state.inMainTouched } : {}
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
 * The transaction an expense or an income tab holds, or why it cannot be sent (`vars` fill the words of the error). `remember` is the account to
 * start the next new row in ('' when the form has no account field). An edit says nothing of the account it keeps (it may be an archived one).
 *
 * In an account in another currency the first field is what it was in that currency (`nativeAmount`, always sent, also by an edit that stays) and the
 * second what it was in the app's (`amount`); both are needed. In an account in the app's currency there is one amount and no `nativeAmount`, also
 * for a transaction that moves there (the app clears the one it had).
 */
export function transactionBody(
  entry: EntryState,
  accounts: readonly Account[],
  current: Transaction | null,
): { body: TransactionBody; remember: string } | { error: MessageKey; vars?: { code: string } } {
  const name = entry.name.trim()
  const categoryId = entry.categoryId
  if (!name || !entry.date || !categoryId) return { error: 'tx.errorRequired' }
  const view = nativeView(accounts, entry, current)
  const nativeAmount = view ? amountString(entry.amount, false) : null
  if (view && nativeAmount === null) return { error: 'entry.errorNative', vars: { code: view.code } }
  const amount = amountString(view ? view.inMain : entry.amount, false)
  if (amount === null) return view ? { error: 'entry.errorNative', vars: { code: view.mainCode } } : { error: 'tx.errorAmount' }
  const account = accountField(accounts, entry.tab === 'INCOME' ? 'INCOME' : 'EXPENSE', entry.account, current?.accountId)
  if (account.missing) return { error: 'entry.errorAccount' }
  const description = entry.text.trim()
  return {
    body: {
      name,
      amount,
      ...(nativeAmount !== null ? { nativeAmount } : {}),
      paymentDate: entry.date,
      categoryId,
      description: description === '' ? null : description,
      ...(account.send ? { accountId: account.send } : {}),
    },
    remember: account.shown ? account.value : '',
  }
}
