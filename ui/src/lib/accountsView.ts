import type { Account, AccountLine } from '../api/accounts'
import type { MessageKey } from '../i18n/en'
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

/**
 * The rule that hides it all: more than one account that is not archived. When it is false no form has an account field, the
 * transactions have no account filter or column, and nothing asks about accounts.
 */
export function several(accounts: readonly Account[] | null): boolean {
  return accounts !== null && activeAccounts(accounts).length > 1
}

/** The account a new row starts with: the last one used if it is still there and not archived, else the first in the list. '' with no account. */
export function defaultAccountId(accounts: readonly Account[]): string {
  const active = activeAccounts(accounts)
  const last = storedId(LAST)
  return active.find((account) => account.id === last)?.id ?? active[0]?.id ?? ''
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
 * The choices of an account field: the accounts that are not archived, and before them each of `keep` that is archived (the account
 * a row or a side of a transfer already has), so that it shows and stays unless it is changed.
 */
export function accountOptions(accounts: readonly Account[], t: Translate, keep: readonly (string | undefined)[] = []): { value: string; label: string }[] {
  const kept = accounts.filter((account) => account.archived && keep.includes(account.id))
  return [...kept, ...activeAccounts(accounts)].map((account) => ({ value: account.id, label: accountLabel(account, t) }))
}

/**
 * The account field of a form. `chosen` is what the person picked (null: nothing yet), `current` the account the row already has (none
 * for a new row). `shown`: whether the form has the field at all. `value`: what it holds. `send`: the account to send, or undefined
 * when there is nothing to say: no field, or the account the row already has.
 */
export function accountChoice(accounts: readonly Account[] | null, chosen: string | null, current?: string): { shown: boolean; value: string; send: string | undefined } {
  const shown = several(accounts)
  const value = chosen ?? current ?? (accounts ? defaultAccountId(accounts) : '')
  return { shown, value, send: shown && value !== '' && value !== current ? value : undefined }
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

// -- the tabs of the page ---------------------------------------------------------------------------------------------------------

export const ACCOUNT_TABS = ['accounts', 'income'] as const
export type AccountTab = (typeof ACCOUNT_TABS)[number]

/** The tab an address names: `?tab=income` is the income, anything else (no tab, a word nobody knows) the accounts. Other pages link to this. */
export function accountsTab(search: string | URLSearchParams): AccountTab {
  return new URLSearchParams(search).get('tab') === 'income' ? 'income' : 'accounts'
}

/** The query of a tab: none for the accounts, the page's own address. */
export function tabQuery(tab: AccountTab): Record<string, string> {
  return tab === 'income' ? { tab } : {}
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

/** How wide the bar of an amount is, as a percentage of the track: the largest amount of the list fills it. No largest amount, no bar. */
export function barWidth(amount: string, largest: string): number {
  const most = Number(largest)
  const value = Number(amount)
  return most > 0 && value > 0 ? Math.min(100, (value / most) * 100) : 0
}
