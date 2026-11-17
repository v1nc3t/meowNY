import type { Account, AccountKind, AccountPatch, NewAccount } from '../api/accounts'
import { cleanCategoryName } from '../api/categories'
import type { NewTransfer, Transfer, TransferPatch } from '../api/transfers'
import type { MessageKey } from '../i18n/en'
import { accountById, activeAccounts, currencyKey, defaultAccountId, rateOf } from './accountsView'
import { amountString, convertBetween, parseRate, signedAmountString } from './money'

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

/** The most characters of a BIC, a group name and the notes (docs/accounts-design.md, section 20.2). The name's is `NAME_MAX`. */
export const BIC_MAX = 20
export const GROUP_MAX = 50
export const NOTES_MAX = 1000

export type AccountFields = {
  name: string
  kind: AccountKind
  number: string
  balance: string
  date: string
  bic: string
  group: string
  /** Empty means 0.00. */
  virtualBalance: string
  includeInNetWorth: boolean
  notes: string
  /** The currency as typed (in capitals), three letters. */
  currency: string
  /** The rate as typed, of an account that is not in the app's currency. */
  rate: string
  /** What the account should hold today, in its currency; empty means nothing to set. Only a change has it (docs/accounts-design.md, section 21.5). */
  balanceToday: string
}

/** Whether the account has any of the five details that sit behind "more": the dialog then opens with them shown. A new account has none. */
export function hasDetails(account: Account | null): boolean {
  return account !== null && (account.bic !== null || account.notes !== null || account.group !== null || !account.includeInNetWorth || Number(account.virtualBalance) !== 0)
}

/**
 * The fields the dialog opens with: empty with the defaults for a new account (`shownName` is not used), the row for one that exists.
 * `shownName` is the name as the lists show it (the main account in the person's language). No opening balance reads as an empty
 * field, and then the date starts at today: the stored one says nothing (the main account's is 1970). No virtual balance is an empty field.
 * The currency starts as the app's (`main`) for a new account and as the account's own for one that exists; "balance today" starts empty.
 */
export function accountFields(account: Account | null, shownName: string, today: string, main: string): AccountFields {
  if (!account) {
    return {
      name: '',
      kind: 'CHECKING',
      number: '',
      balance: '',
      date: today,
      bic: '',
      group: '',
      virtualBalance: '',
      includeInNetWorth: true,
      notes: '',
      currency: main.toUpperCase(),
      rate: '',
      balanceToday: '',
    }
  }
  const none = account.openingBalance === '0.00'
  return {
    name: shownName,
    kind: account.kind,
    number: account.number ?? '',
    balance: none ? '' : account.openingBalance,
    date: none ? today : account.openingDate,
    bic: account.bic ?? '',
    group: account.group ?? '',
    virtualBalance: Number(account.virtualBalance) === 0 ? '' : account.virtualBalance,
    includeInNetWorth: account.includeInNetWorth,
    notes: account.notes ?? '',
    currency: account.currency.toUpperCase(),
    rate: account.rate ?? '',
    balanceToday: '',
  }
}

// -- the currency of an account -------------------------------------------------------------------------------------------------

/** Why the currency of an account cannot be changed: it is the main account (always in the app's currency), or something points at it. */
export type CurrencyLock = 'main' | 'entries' | null

/** Whether the currency field is shut, and why. A new account's is open. */
export function currencyLock(account: Account | null): CurrencyLock {
  if (!account) return null
  return account.isMain ? 'main' : account.entries > 0 ? 'entries' : null
}

/** What was typed as a currency, as a code: trimmed, in capitals, exactly three letters; else null. */
export function cleanCurrency(text: string): string | null {
  const code = text.trim().toUpperCase()
  return /^[A-Z]{3}$/.test(code) ? code : null
}

/** Whether a code is the app's own currency, in any case. */
export function isMainCurrency(code: string, main: string): boolean {
  return code.trim().toUpperCase() === main.trim().toUpperCase()
}

/**
 * The code the dialog shows beside the amounts it takes (opening, virtual, balance today): the currency typed, once it is three letters and not
 * the app's own. Null otherwise, and the labels say nothing.
 */
export function foreignCode(fields: Pick<AccountFields, 'currency'>, main: string): string | null {
  const code = cleanCurrency(fields.currency)
  return code !== null && !isMainCurrency(code, main) ? code : null
}

/**
 * Whether the rate row shows, and the code it names: the currency typed so far (the first letter is enough, so the row does not come and go
 * with each letter) when it is not the app's own. An account whose currency is shut shows its rate when it is in another currency, and never
 * for the main account. Null: no row.
 */
export function rateRow(account: Account | null, fields: Pick<AccountFields, 'currency'>, main: string): { code: string } | null {
  const lock = currencyLock(account)
  if (lock === 'main') return null
  if (lock === 'entries') return account?.foreign ? { code: account.currency.toUpperCase() } : null
  const typed = fields.currency.trim().toUpperCase()
  return typed === '' || isMainCurrency(typed, main) ? null : { code: typed }
}

/** Whether two typed rates are the same number ("5.0" and "5.00"). A rate that is not one is never the same as anything. */
function sameRate(a: string | null, b: string): boolean {
  const first = a === null ? null : parseRate(a)
  const second = parseRate(b)
  return first !== null && second !== null && first.numerator * second.denominator === second.numerator * first.denominator
}

/**
 * "Balance today" was typed: the opening balance field is shut while it is filled, so it goes back to what it holds (what is shown is what is
 * used), and emptied again it works as before. `start` is what the dialog opened with.
 */
export function withBalanceToday(fields: AccountFields, start: AccountFields, text: string): AccountFields {
  return text.trim() === '' ? { ...fields, balanceToday: text } : { ...fields, balanceToday: text, balance: start.balance }
}

/** Whether "balance today" is filled: the opening balance is then not used (they cannot be sent together). */
export function balanceTodayFilled(fields: Pick<AccountFields, 'balanceToday'>): boolean {
  return fields.balanceToday.trim() !== ''
}

/** Trimmed text of at most `max` characters with no control character (a line break is none when `lineBreaks`), or null. Empty is fine: it means nothing there. */
function clean(input: string, max: number, lineBreaks = false): string | null {
  const text = input.trim()
  const chars = [...text]
  return chars.length > max || chars.some((char) => /\p{Cc}/u.test(char) && !(lineBreaks && char === '\n')) ? null : text
}

export type AccountRequest = { create: NewAccount } | { patch: AccountPatch }

/**
 * A submit of the account dialog. `account` and `start` (the fields it opened with) are null for a new account. An edit sends only what
 * changed, and `null` for a BIC, group or notes that was emptied (spaces alone are empty). A name is compared with the one shown at the
 * start, so the main account is not renamed to its own translation. The date of an opening balance of 0.00 says nothing and is not sent.
 * `main` is the app's currency.
 *
 * The currency: a new account in the app's currency (any case) sends none, another code sends it with its rate (required, above 0). An edit
 * sends a currency only when it changed, and one that goes back to the app's currency is sent as the app's code, with no rate; the rate of an
 * account in another currency is sent when it changed (also when its currency is shut), and when the currency changed. The currency of the main
 * account and of an account with entries is never sent. "Balance today", when filled, is sent as `balance` and the opening balance is not.
 */
export function accountSubmit(state: DialogState, account: Account | null, start: AccountFields | null, fields: AccountFields, main: string): Step<AccountRequest> {
  if (state !== 'EDITING') return { do: 'ignore' }
  const name = cleanCategoryName(fields.name)
  if (name === null) return { do: 'refuse', error: 'accounts.nameError' }
  const lock = currencyLock(account)
  // the currency the account ends in (a shut field keeps the account's own) and whether that is another one than the app's
  const code = lock === null ? cleanCurrency(fields.currency) : (account?.currency.toUpperCase() ?? '')
  if (code === null) return { do: 'refuse', error: 'accounts.currencyError' }
  const foreign = lock === 'main' ? false : lock === 'entries' ? (account?.foreign ?? false) : !isMainCurrency(code, main)
  const rate = fields.rate.trim()
  if (foreign && parseRate(rate) === null) return { do: 'refuse', error: 'accounts.rateError' }
  // "balance today" is for an account that exists; filled, it takes the place of the opening balance
  const usesToday = account !== null && balanceTodayFilled(fields)
  const today = usesToday ? signedAmountString(fields.balanceToday) : null
  if (usesToday && today === null) return { do: 'refuse', error: 'accounts.balanceError' }
  const balance = usesToday ? (account?.openingBalance ?? '0.00') : fields.balance.trim() === '' ? '0.00' : signedAmountString(fields.balance)
  if (balance === null) return { do: 'refuse', error: 'accounts.balanceError' }
  const virtual = fields.virtualBalance.trim() === '' ? '0.00' : signedAmountString(fields.virtualBalance)
  if (virtual === null) return { do: 'refuse', error: 'accounts.balanceError' }
  const bic = clean(fields.bic, BIC_MAX)
  if (bic === null) return { do: 'refuse', error: 'accounts.bicError' }
  const group = clean(fields.group, GROUP_MAX)
  if (group === null) return { do: 'refuse', error: 'accounts.groupError' }
  const notes = clean(fields.notes, NOTES_MAX, true)
  if (notes === null) return { do: 'refuse', error: 'accounts.notesError' }
  const number = fields.number.trim()
  if (!account) {
    return {
      do: 'send',
      request: {
        create: {
          name,
          kind: fields.kind,
          ...(number ? { number } : {}),
          ...(balance !== '0.00' ? { openingBalance: balance, openingDate: fields.date } : {}),
          ...(bic ? { bic } : {}),
          ...(group ? { group } : {}),
          ...(notes ? { notes } : {}),
          ...(fields.includeInNetWorth ? {} : { includeInNetWorth: false }),
          ...(virtual !== '0.00' ? { virtualBalance: virtual } : {}),
          ...(foreign ? { currency: code, rate } : {}),
        },
      },
    }
  }
  const patch: AccountPatch = {}
  if (name !== cleanCategoryName(start?.name ?? account.name)) patch.name = name
  if (fields.kind !== account.kind) patch.kind = fields.kind
  if (number !== (account.number ?? '')) patch.number = number === '' ? null : number
  if (!usesToday && balance !== account.openingBalance) patch.openingBalance = balance
  if (balance !== '0.00' && fields.date !== account.openingDate) patch.openingDate = fields.date
  if (today !== null && today !== account.balance) patch.balance = today
  if (bic !== (account.bic ?? '')) patch.bic = bic === '' ? null : bic
  if (group !== (account.group ?? '')) patch.group = group === '' ? null : group
  if (notes !== (account.notes ?? '')) patch.notes = notes === '' ? null : notes
  if (fields.includeInNetWorth !== account.includeInNetWorth) patch.includeInNetWorth = fields.includeInNetWorth
  if (virtual !== account.virtualBalance) patch.virtualBalance = virtual
  if (lock !== 'main') {
    // a currency typed is a change when it is not the account's own; one that goes back to the app's currency is sent as the app's own code, and the app clears the rate
    const changed = lock === null && code !== account.currency.toUpperCase()
    if (foreign) {
      if (changed) patch.currency = code
      if (changed || !sameRate(account.rate, rate)) patch.rate = rate
    } else if (changed) {
      patch.currency = main
    }
  }
  return Object.keys(patch).length === 0 ? { do: 'close' } : { do: 'send', request: { patch } }
}

// -- the transfer dialog --------------------------------------------------------------------------------------------------------

export type Sides = { from: string; to: string }
export type TransferFields = Sides & {
  /** What leaves the first account, in its currency, as typed. */
  amount: string
  date: string
  note: string
  /** What reaches the second account, in its currency, as typed: only used when the two accounts are in different currencies. */
  arrives: string
  /**
   * The person typed `arrives` (it is theirs and stays); not yet, or they emptied it again: it follows the rates, and `arrives` is not used. Opening a
   * transfer that has an amount of arrival counts as typed.
   */
  arrivesTouched: boolean
}

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

/** The fields of a transfer that is not typed yet: the amounts and the note empty, the day given. */
export function transferFieldsOf(sides: Sides, date: string): TransferFields {
  return { ...sides, amount: '', date, note: '', arrives: '', arrivesTouched: false }
}

/** The fields a transfer being changed opens with: the amount of arrival it has counts as typed. */
export function transferFieldsOfRow(transfer: Transfer): TransferFields {
  return {
    from: transfer.fromAccountId,
    to: transfer.toAccountId,
    amount: transfer.amount,
    date: transfer.date,
    note: transfer.note ?? '',
    arrives: transfer.toAmount ?? '',
    arrivesTouched: transfer.toAmount !== null,
  }
}

/**
 * The currencies of a transfer's two sides, for the labels. `arrives` is the code of the second account when the two are in different
 * currencies (then a second amount is needed: what arrives); null when they are in the same one (one amount, nothing sent as `toAmount`).
 * `amount` is the code of the first account when its amount is not in the app's currency (the account is in another one, or the two differ),
 * so that the field says what it is; null for an account in the app's currency whose partner is in it too, as before.
 */
export function transferCurrencies(accounts: readonly Account[], sides: Sides): { amount: string | null; arrives: string | null } {
  const from = accountById(accounts, sides.from)
  const to = accountById(accounts, sides.to)
  const differ = from !== null && to !== null && currencyKey(from) !== currencyKey(to)
  return { amount: from && (from.foreign || differ) ? from.currency.toUpperCase() : null, arrives: differ ? (to?.currency.toUpperCase() ?? null) : null }
}

/**
 * What the second account would get for the amount typed: through the app's currency at the two rates (an account in the app's currency has
 * rate 1), rounded half up to the cent. Null when the two sides are in the same currency, or the amount is not one.
 */
export function suggestedArrival(accounts: readonly Account[], fields: Pick<TransferFields, 'from' | 'to' | 'amount'>): string | null {
  if (transferCurrencies(accounts, fields).arrives === null) return null
  const amount = amountString(fields.amount, false)
  return amount === null ? null : convertBetween(amount, rateOf(accountById(accounts, fields.from)), rateOf(accountById(accounts, fields.to)))
}

/** What the second amount field holds: what the person typed in it, or until then (and after they empty it) what the rates give; empty when neither. */
export function arrivalText(accounts: readonly Account[], fields: Pick<TransferFields, 'from' | 'to' | 'amount' | 'arrives' | 'arrivesTouched'>): string {
  return fields.arrivesTouched ? fields.arrives : (suggestedArrival(accounts, fields) ?? '')
}

/** The second amount was typed in: it is theirs from now on; emptied, it follows the rates again. */
export function withArrival<T extends Pick<TransferFields, 'arrives' | 'arrivesTouched'>>(fields: T, text: string): T {
  return { ...fields, arrives: text, arrivesTouched: text.trim() !== '' }
}

export type TransferRequest = { create: NewTransfer } | { patch: TransferPatch }

/**
 * A submit of the transfer dialog; `transfer` is null for a new one. An edit sends only what changed. When the two sides are in different
 * currencies the amount of arrival is needed (what the second field holds, typed or from the rates) and is sent as `toAmount`; for a transfer
 * being changed it is sent when it is new to the row or changed. Two sides in the same currency send none: the app clears a stored one.
 */
export function transferSubmit(state: DialogState, transfer: Transfer | null, fields: TransferFields, accounts: readonly Account[]): Step<TransferRequest> {
  if (state !== 'EDITING') return { do: 'ignore' }
  if (!fields.from || !fields.to || fields.from === fields.to) return { do: 'refuse', error: 'transfers.errorAccounts' }
  const amount = amountString(fields.amount, false)
  if (amount === null) return { do: 'refuse', error: 'tx.errorAmount' }
  const needsArrival = transferCurrencies(accounts, fields).arrives !== null
  const toAmount = needsArrival ? amountString(arrivalText(accounts, fields), false) : null
  if (needsArrival && toAmount === null) return { do: 'refuse', error: 'transfers.errorArrives' }
  const note = fields.note.trim()
  if (!transfer) {
    return {
      do: 'send',
      request: { create: { fromAccountId: fields.from, toAccountId: fields.to, amount, date: fields.date, ...(note ? { note } : {}), ...(toAmount !== null ? { toAmount } : {}) } },
    }
  }
  const patch: TransferPatch = {}
  if (fields.from !== transfer.fromAccountId) patch.fromAccountId = fields.from
  if (fields.to !== transfer.toAccountId) patch.toAccountId = fields.to
  if (amount !== transfer.amount) patch.amount = amount
  if (fields.date !== transfer.date) patch.date = fields.date
  if (note !== (transfer.note ?? '')) patch.note = note === '' ? null : note
  if (toAmount !== null && toAmount !== transfer.toAmount) patch.toAmount = toAmount
  return Object.keys(patch).length === 0 ? { do: 'close' } : { do: 'send', request: { patch } }
}
