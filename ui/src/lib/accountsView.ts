import type { Account, AccountLine } from '../api/accounts'
import type { MessageKey } from '../i18n/en'
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
