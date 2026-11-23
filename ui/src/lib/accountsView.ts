import { isLiability, type Account, type AccountLine } from '../api/accounts'
import type { MessageKey } from '../i18n/en'
import { fromMinor, toMinor } from './money'
import { applyOrder } from './reorder'
import { saveChoice, storedId } from './sticky'

// What the screens show of the accounts (docs/accounts-design.md, section 13). Nothing here adds money up.

type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string

/** The name the main account has until the person renames it. The app stores it in English on every device. */
export const MAIN_NAME = 'Main account'

/** Where the last account a row was saved in is kept (an id). */
const LAST = 'account.last'

/** The accounts that are not archived, in the person's order. */
export function activeAccounts(accounts: readonly Account[]): Account[] {
  return accounts.filter((account) => !account.archived)
}

/** The accounts that hold money and are not archived: every one that is not a credit, a loan or a mortgage (docs/loans-deposits-design.md, section 10). */
export function assetAccounts(accounts: readonly Account[]): Account[] {
  return activeAccounts(accounts).filter((account) => !isLiability(account.kind))
}

/**
 * The accounts a field of a row can offer (an expense, an income, a recurring item, a friend's entry, a stash): the assets that are not archived,
 * and with `mainOnly` (the recurring items and the debts, which live only in the app's currency: docs/accounts-design.md, section 21.1) only those
 * in the app's currency. A liability is never one: a transfer is what moves money to it.
 */
export function listable(accounts: readonly Account[], mainOnly = false): Account[] {
  return assetAccounts(accounts).filter((account) => !(mainOnly && account.foreign))
}

/**
 * The rule that hides the account field: more than one asset account that is not archived. When it is false no form of a row has an account field.
 * With `mainOnly` only the accounts such a field would list count: a form that offers one account has nothing to ask. A loan does not count: with
 * one asset and a loan the field stays hidden.
 */
export function several(accounts: readonly Account[] | null, mainOnly = false): boolean {
  return accounts !== null && listable(accounts, mainOnly).length > 1
}

/**
 * More than one account that is not archived, of any kind: what a transfer needs, and what the transactions page asks before it has an account
 * filter, a column and a tab of transfers (the payment of a loan is one).
 */
export function manyAccounts(accounts: readonly Account[] | null): boolean {
  return accounts !== null && activeAccounts(accounts).length > 1
}

/**
 * The account a new row starts with: the last one used if it is still there and not archived (and, with `mainOnly`, in the app's currency),
 * else the first in the list. '' with no account.
 */
export function defaultAccountId(accounts: readonly Account[], mainOnly = false): string {
  const choices = listable(accounts, mainOnly)
  const last = storedId(LAST)
  return choices.find((account) => account.id === last)?.id ?? choices[0]?.id ?? ''
}

/** Remembers the account a row was saved in, for the next new row. */
export function rememberAccount(id: string): void {
  if (id) saveChoice(LAST, id)
}

/** The account's name. The main account reads in the person's language until it is renamed; any rename shows as typed. */
export function accountName(account: Pick<Account, 'name' | 'isMain'>, t: Translate): string {
  return account.isMain && account.name === MAIN_NAME ? t('accounts.mainName') : account.name
}

/** The name as a form shows it: an archived account says so. */
export function accountLabel(account: Pick<Account, 'name' | 'isMain' | 'archived'>, t: Translate): string {
  const name = accountName(account, t)
  return account.archived ? t('accounts.archivedSuffix', { name }) : name
}

/**
 * The choices of an account field: the accounts that are not archived (with `mainOnly`, those in the app's currency), and before them each of
 * `keep` that the field would not list otherwise (the account a row or a side of a transfer already has: archived, or in another currency
 * where only the app's is offered), so that it shows and stays unless it is changed.
 */
export function accountOptions(
  accounts: readonly Account[],
  t: Translate,
  keep: readonly (string | undefined)[] = [],
  mainOnly = false,
): { value: string; label: string }[] {
  return choicesOf(listable(accounts, mainOnly), accounts, t, keep)
}

/** The choices of a transfer's two sides: every account that is not archived, a liability too (that is how money reaches one), and the `keep` ones as above. */
export function transferOptions(accounts: readonly Account[], t: Translate, keep: readonly (string | undefined)[] = []): { value: string; label: string }[] {
  return choicesOf(activeAccounts(accounts), accounts, t, keep)
}

function choicesOf(listed: readonly Account[], accounts: readonly Account[], t: Translate, keep: readonly (string | undefined)[]): { value: string; label: string }[] {
  const kept = accounts.filter((account) => keep.includes(account.id) && !listed.includes(account))
  return [...kept, ...listed].map((account) => ({ value: account.id, label: accountLabel(account, t) }))
}

/**
 * The account field of a form. `chosen` is what the person picked (null: nothing yet), `current` the account the row already has (none
 * for a new row). `shown`: whether the form has the field at all. `value`: what it holds. `send`: the account to send, or undefined
 * when there is nothing to say: no field, or the account the row already has. `mainOnly`: the form offers only accounts in the app's currency.
 */
export function accountChoice(
  accounts: readonly Account[] | null,
  chosen: string | null,
  current?: string,
  mainOnly = false,
): { shown: boolean; value: string; send: string | undefined } {
  const shown = several(accounts, mainOnly)
  const value = chosen ?? current ?? (accounts ? defaultAccountId(accounts, mainOnly) : '')
  return { shown, value, send: shown && value !== '' && value !== current ? value : undefined }
}

// -- the currency of an account ------------------------------------------------------------------------------------------------------

/** The account with this id, if there is one. */
export function accountById(accounts: readonly Account[] | null, id: string): Account | null {
  return accounts?.find((account) => account.id === id) ?? null
}

/** The app's currency: the one the main account is in (it is never in another). '' while there is no account to say. */
export function mainCurrency(accounts: readonly Account[] | null): string {
  return (accounts?.find((account) => account.isMain) ?? accounts?.find((account) => !account.foreign))?.currency ?? ''
}

/**
 * What an account's currency is for comparing: '' for an account in the app's currency, else its code in capitals. Two accounts are in
 * different currencies exactly when their keys differ (the app compares them so: an account in the app's own currency that was made as a
 * foreign one on another device is still a foreign account).
 */
export function currencyKey(account: Pick<Account, 'foreign' | 'currency'> | null): string {
  return account?.foreign ? account.currency.toUpperCase() : ''
}

/** The rate of an account for turning amounts through the app's currency: null for one in the app's currency (rate 1). */
export function rateOf(account: Pick<Account, 'foreign' | 'rate'> | null): string | null {
  return account?.foreign ? account.rate : null
}

/**
 * The words of a line of an account's statement: what it was, and who or what it was with (a category, the other account, a friend).
 * A line with no words of its own (a transfer or a debt without a note) is named by the other side, which then is not said twice.
 */
export function lineWords(line: Pick<AccountLine, 'source' | 'what' | 'other'>, t: Translate): { what: string; other: string } {
  const other =
    line.source === 'TRANSFER_IN'
      ? t('transfers.in', { name: line.other ?? '' })
      : line.source === 'TRANSFER_OUT'
        ? t('transfers.out', { name: line.other ?? '' })
        : (line.other ?? '')
  const what = line.source === 'OPENING' ? t('accounts.opening') : line.what
  return what === '' ? { what: other, other: '' } : { what, other }
}

/** A signed amount for a list: "+12.50" in green, "-12.50" in red, nothing at all plain. `format` writes the amount without its sign. */
export function signedView(amount: string, format: (unsigned: string) => string): { text: string; tone: '' | 'is-income' | 'is-expense' } {
  const unsigned = amount.replace('-', '')
  if (Number(unsigned) === 0) return { text: format(unsigned), tone: '' }
  return amount.startsWith('-') ? { text: `-${format(unsigned)}`, tone: 'is-expense' } : { text: `+${format(unsigned)}`, tone: 'is-income' }
}

// -- the address and the name of a row -----------------------------------------------------------------------------------------------

/**
 * Whether an address is the old one of the income tab (`?tab=income`) that the accounts page had for a while: the income is on the
 * categories page again (docs/accounts-design.md, section 20.1), and a link of that kind goes there. Nothing else is: no tab, another word.
 */
export function oldIncomeAddress(search: string | URLSearchParams): boolean {
  return new URLSearchParams(search).get('tab') === 'income'
}

/**
 * The small words under the name of an account in a list: that the net worth leaves it out, else none. An account in another currency
 * says nothing more here: its currency is in its balance ("RON 0.32"), and what it is worth in the app's under that.
 */
export function nameMark(account: Pick<Account, 'includeInNetWorth'>, t: Translate): string {
  return account.includeInNetWorth ? '' : t('accounts.notInNetWorth')
}

// -- groups -----------------------------------------------------------------------------------------------------------------------

/** A run of accounts under one heading: `name` is the group's, or null for the accounts that have none (and for the one list of no groups at all). */
export type AccountGroup = { name: string | null; accounts: Account[] }

/** Whether any account that is not archived has a group: only then the list has headings. */
export function hasGroups(accounts: readonly Account[]): boolean {
  return activeAccounts(accounts).some((account) => account.group)
}

/**
 * The accounts that are not archived, under their groups: the groups in the order of their first account in the list, the accounts of
 * each in the list's order, those with no group last. With no group in use, one list with no name. An archived account is never in one.
 */
export function groupAccounts(accounts: readonly Account[]): AccountGroup[] {
  const active = activeAccounts(accounts)
  if (!hasGroups(active)) return [{ name: null, accounts: active }]
  const named = new Map<string, Account[]>()
  const none: Account[] = []
  for (const account of active) {
    if (!account.group) none.push(account)
    else named.set(account.group, [...(named.get(account.group) ?? []), account])
  }
  return [...[...named].map(([name, list]) => ({ name, accounts: list })), ...(none.length > 0 ? [{ name: null, accounts: none }] : [])]
}

/**
 * The ids of all accounts that are not archived, in the order to send after the accounts of one group were put in the order `ids`
 * (`group` null is the accounts with no group). Those accounts take the places that the group's accounts had, so nothing outside the
 * group moves. An id that is not in the group (another group's, an archived account's, an unknown one) and an id said twice count once or not at all.
 */
export function reorderWithinGroup(accounts: readonly Account[], group: string | null, ids: readonly string[]): string[] {
  const active = activeAccounts(accounts)
  const members = new Set(active.filter((account) => (account.group || null) === group).map((account) => account.id))
  return applyOrder(active, [...new Set(ids)].filter((id) => members.has(id))).map((account) => account.id)
}

/** Whether a row says what is available: only when there is a virtual balance (a card limit), "-0.00" being none. */
export function showAvailable(account: Pick<Account, 'virtualBalance'>): boolean {
  return Number(account.virtualBalance) !== 0
}

/**
 * What an account can spend, for its "free" column: the balance minus what its stashes hold (`free`); with a virtual balance (a card's limit)
 * what is available, the same less what its stashes hold. In the account's own currency.
 */
export function freeOf(account: Pick<Account, 'free' | 'virtualBalance'>): string {
  if (!showAvailable(account)) return account.free
  return fromMinor((toMinor(account.free) ?? 0n) + (toMinor(account.virtualBalance) ?? 0n))
}

/** How wide the bar of an amount is, as a percentage of the track: the largest amount of the list fills it. No largest amount, no bar. */
export function barWidth(amount: string, largest: string): number {
  const most = Number(largest)
  const value = Number(amount)
  return most > 0 && value > 0 ? Math.min(100, (value / most) * 100) : 0
}
