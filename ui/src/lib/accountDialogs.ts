import type { Account, AccountKind, AccountPatch, NewAccount } from '../api/accounts'
import { cleanCategoryName } from '../api/categories'
import type { NewTransfer, Transfer, TransferPatch } from '../api/transfers'
import type { MessageKey } from '../i18n/en'
import { activeAccounts, defaultAccountId } from './accountsView'
import { amountString, signedAmountString } from './money'

// What the account dialog and the transfer dialog decide, apart from drawing (docs/accounts-design.md, section 5.4).

/** Where a dialog is: open with its fields, waiting for the app's answer, or gone. */
export type DialogState = 'EDITING' | 'SAVING' | 'CLOSED'
/** `send`: a request went out. `ok` and `error`: its answer. `cancel`: cancel, Esc or a click outside. */
export type DialogEvent = 'send' | 'ok' | 'error' | 'cancel'

/**
 * The next state. A second submit while saving changes nothing; an error gives the fields back (they are never cleared); cancel
 * closes even while saving (the request still finishes, and the page reloads its list then).
 */
export function dialogNext(state: DialogState, event: DialogEvent): DialogState {
  if (state === 'CLOSED' || event === 'cancel') return 'CLOSED'
  if (state === 'EDITING') return event === 'send' ? 'SAVING' : 'EDITING'
  return event === 'ok' ? 'CLOSED' : event === 'error' ? 'EDITING' : 'SAVING'
}

/** What a submit does: nothing (already saving), an error toast with nothing sent, a close with nothing sent (nothing changed), or a request. */
export type Step<T> = { do: 'ignore' } | { do: 'refuse'; error: MessageKey } | { do: 'close' } | { do: 'send'; request: T }

// -- the account dialog ---------------------------------------------------------------------------------------------------------

export type AccountFields = { name: string; kind: AccountKind; number: string; balance: string; date: string }

/**
 * The fields the dialog opens with: empty with the defaults for a new account (`shownName` is not used), the row for one that exists.
 * `shownName` is the name as the lists show it (the main account in the person's language). No opening balance reads as an empty
 * field, and then the date starts at today: the stored one says nothing (the main account's is 1970).
 */
export function accountFields(account: Account | null, shownName: string, today: string): AccountFields {
  if (!account) return { name: '', kind: 'CHECKING', number: '', balance: '', date: today }
  const none = account.openingBalance === '0.00'
  return { name: shownName, kind: account.kind, number: account.number ?? '', balance: none ? '' : account.openingBalance, date: none ? today : account.openingDate }
}

export type AccountRequest = { create: NewAccount } | { patch: AccountPatch }

/**
 * A submit of the account dialog. `account` and `start` (the fields it opened with) are null for a new account. An edit sends only what
 * changed. A name is compared with the one shown at the start, so the main account is not renamed to its own translation. The date of
 * an opening balance of 0.00 says nothing and is not sent.
 */
export function accountSubmit(state: DialogState, account: Account | null, start: AccountFields | null, fields: AccountFields): Step<AccountRequest> {
  if (state !== 'EDITING') return { do: 'ignore' }
  const name = cleanCategoryName(fields.name)
  if (name === null) return { do: 'refuse', error: 'accounts.nameError' }
  const balance = fields.balance.trim() === '' ? '0.00' : signedAmountString(fields.balance)
  if (balance === null) return { do: 'refuse', error: 'accounts.balanceError' }
  const number = fields.number.trim()
  if (!account) {
    return {
      do: 'send',
      request: { create: { name, kind: fields.kind, ...(number ? { number } : {}), ...(balance !== '0.00' ? { openingBalance: balance, openingDate: fields.date } : {}) } },
    }
  }
  const patch: AccountPatch = {}
  if (name !== cleanCategoryName(start?.name ?? account.name)) patch.name = name
  if (fields.kind !== account.kind) patch.kind = fields.kind
  if (number !== (account.number ?? '')) patch.number = number === '' ? null : number
  if (balance !== account.openingBalance) patch.openingBalance = balance
  if (balance !== '0.00' && fields.date !== account.openingDate) patch.openingDate = fields.date
  return Object.keys(patch).length === 0 ? { do: 'close' } : { do: 'send', request: { patch } }
}

// -- the transfer dialog --------------------------------------------------------------------------------------------------------

export type Sides = { from: string; to: string }
export type TransferFields = Sides & { amount: string; date: string; note: string }

/**
 * The two accounts a new transfer starts with: from the account of the page when it is one that is not archived, else the one a new row
 * starts with; to the first other one. Either is '' when there is no such account.
 */
export function transferStart(accounts: readonly Account[], pageAccountId?: string): Sides {
  const active = activeAccounts(accounts)
  const from = active.find((account) => account.id === pageAccountId)?.id ?? defaultAccountId(accounts)
  return { from, to: active.find((account) => account.id !== from)?.id ?? '' }
}

/** Choosing in one list the account the other holds swaps the two, so the same account on both sides cannot be made. */
export function chooseSide(sides: Sides, side: 'from' | 'to', id: string): Sides {
  const other = side === 'from' ? 'to' : 'from'
  return sides[other] === id ? ({ [side]: id, [other]: sides[side] } as Sides) : { ...sides, [side]: id }
}

export type TransferRequest = { create: NewTransfer } | { patch: TransferPatch }

/** A submit of the transfer dialog; `transfer` is null for a new one. An edit sends only what changed. */
export function transferSubmit(state: DialogState, transfer: Transfer | null, fields: TransferFields): Step<TransferRequest> {
  if (state !== 'EDITING') return { do: 'ignore' }
  if (!fields.from || !fields.to || fields.from === fields.to) return { do: 'refuse', error: 'transfers.errorAccounts' }
  const amount = amountString(fields.amount, false)
  if (amount === null) return { do: 'refuse', error: 'tx.errorAmount' }
  const note = fields.note.trim()
  if (!transfer) {
    return { do: 'send', request: { create: { fromAccountId: fields.from, toAccountId: fields.to, amount, date: fields.date, ...(note ? { note } : {}) } } }
  }
  const patch: TransferPatch = {}
  if (fields.from !== transfer.fromAccountId) patch.fromAccountId = fields.from
  if (fields.to !== transfer.toAccountId) patch.toAccountId = fields.to
  if (amount !== transfer.amount) patch.amount = amount
  if (fields.date !== transfer.date) patch.date = fields.date
  if (note !== (transfer.note ?? '')) patch.note = note === '' ? null : note
  return Object.keys(patch).length === 0 ? { do: 'close' } : { do: 'send', request: { patch } }
}
