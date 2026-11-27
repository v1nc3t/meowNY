import { isLiability, type Account, type Liability, type Payment } from '../api/accounts'
import type { Category } from '../api/categories'
import type { MessageKey } from '../i18n/en'
import type { DialogState } from './accountDialogs'
import { currencyKey } from './accountsView'
import { amountString, fromMinor, toMinor } from './money'

// What the dialog that pays a credit, a loan or a mortgage decides, apart from drawing (docs/loans-deposits-design.md, sections 5.4 and 5.6). The core
// checks every rule again; this only says it before a request goes out. Money is whole minor units (bigint): no floating point touches it.

/** The most characters of the note of a payment: it goes on the transfer and on the expense, as the note of a transfer is. */
export const PAY_NOTE_MAX = 200

/** The fields of the dialog, as typed. `interestTouched`: the person typed the interest, so it stays where it is when the amount changes. */
export type PayFields = { from: string; amount: string; interest: string; interestTouched: boolean; category: string; date: string; note: string }

/** Minor units of an amount; a text that is not an amount counts as 0 (the app's own numbers never are one). */
const minor = (text: string | null | undefined): bigint => (text == null ? 0n : (toMinor(text) ?? 0n))

/** The accounts the money may come from: assets that are not archived, in the currency of the liability (a payment is one amount in one currency). */
export function payFromChoices(accounts: readonly Account[], liability: Pick<Account, 'foreign' | 'currency'>): Account[] {
  return accounts.filter((account) => !account.archived && !isLiability(account.kind) && currencyKey(account) === currencyKey(liability))
}

/** The account a payment starts from: the last one used to pay when it is still a choice, else the main account, else the first. '' with none. */
export function payFrom(choices: readonly Account[], last: string): string {
  return (choices.find((account) => account.id === last) ?? choices.find((account) => account.isMain) ?? choices[0])?.id ?? ''
}

/** The category a payment starts with: the last one used while it is still an expense category, else none (the interest is the only part that needs one). */
export function payCategory(categories: readonly Pick<Category, 'id' | 'type'>[], last: string): string {
  return categories.find((category) => category.id === last && category.type === 'EXPENSE')?.id ?? ''
}

/**
 * The interest a payment of `amount` starts with: a month's interest on what is owed, but never more than the amount (the rest of a small payment
 * is not interest), "0.00" when the debt has no rate or the amount is no amount yet.
 */
export function interestPrefill(liability: { liability: Pick<Liability, 'interestPerMonth'> | null }, amount: string): string {
  const month = liability.liability?.interestPerMonth ?? null
  const cap = amountString(amount, false)
  if (month === null || cap === null) return '0.00'
  const least = minor(month) < minor(cap) ? minor(month) : minor(cap)
  return fromMinor(least)
}

/** The dialog as it opens: the installment as the amount (empty when there is none), the interest worked out from it, today. `last` is what was chosen last time. */
export function payStart(liability: Account, choices: readonly Account[], categories: readonly Pick<Category, 'id' | 'type'>[], last: { from: string; category: string }, today: string): PayFields {
  const amount = liability.installment ?? ''
  return { from: payFrom(choices, last.from), amount, interest: interestPrefill(liability, amount), interestTouched: false, category: payCategory(categories, last.category), date: today, note: '' }
}

/** The amount was typed: the interest follows it (a month's interest, at most the amount) until the person types the interest themselves. */
export function withAmount(fields: PayFields, liability: Account, text: string): PayFields {
  return { ...fields, amount: text, interest: fields.interestTouched ? fields.interest : interestPrefill(liability, text) }
}

/** The interest was typed: it is theirs from now on. */
export function withInterest(fields: PayFields, text: string): PayFields {
  return { ...fields, interest: text, interestTouched: true }
}

/** `ok` with the numbers of the payment, or the one reason it cannot be made yet. An empty interest is 0. */
export type PayCheck =
  | { ok: true; amount: bigint; interest: bigint; principal: bigint }
  | { ok: false; reason: 'from' | 'amount' | 'interest' | 'overAmount' | 'category' }

/** The amount, the interest and the principal as minor units, or the reason there are none: no amount above 0, an interest that is no amount, one above the amount. */
function figures(fields: Pick<PayFields, 'amount' | 'interest'>): { amount: bigint; interest: bigint; principal: bigint } | { reason: 'amount' | 'interest' | 'overAmount' } {
  const amount = amountString(fields.amount, false)
  if (amount === null) return { reason: 'amount' }
  const interest = fields.interest.trim() === '' ? '0.00' : amountString(fields.interest, true)
  if (interest === null) return { reason: 'interest' }
  const whole = minor(amount)
  const part = minor(interest)
  return part > whole ? { reason: 'overAmount' } : { amount: whole, interest: part, principal: whole - part }
}

/**
 * The principal of what is typed, whatever else is missing: what the line under the amounts says (the principal, and what is owed after it) while the
 * category is still to be chosen. Null for an amount or an interest that is wrong.
 */
export function payPrincipal(fields: Pick<PayFields, 'amount' | 'interest'>): bigint | null {
  const numbers = figures(fields)
  return 'principal' in numbers ? numbers.principal : null
}

/**
 * Whether the payment can be made: an account to pay from, an amount above 0, an interest of 0 or more that is not above the amount (an interest
 * equal to it is a payment of all interest: the principal is 0), and a category when the interest is above 0 (with none it is not asked for). The
 * done button is off while this is not ok.
 */
export function checkPay(fields: Pick<PayFields, 'from' | 'amount' | 'interest' | 'category'>): PayCheck {
  if (fields.from === '') return { ok: false, reason: 'from' }
  const numbers = figures(fields)
  if ('reason' in numbers) return { ok: false, reason: numbers.reason }
  if (numbers.interest > 0n && fields.category === '') return { ok: false, reason: 'category' }
  return { ok: true, ...numbers }
}

/**
 * What the debt is after the principal is paid: `owed` is never below 0, and `state` says when nothing is owed any more (`PAID_OFF`) or more was paid than
 * was owed (`OVERPAID`, allowed: the screens show it). `balance` is the liability's balance today, below 0 while something is owed.
 */
export function payAfter(balance: string, principal: bigint): { owed: bigint; state: Liability['state'] } {
  const after = minor(balance) + principal
  return { owed: after < 0n ? -after : 0n, state: after < 0n ? 'OWING' : after === 0n ? 'PAID_OFF' : 'OVERPAID' }
}

/** What a submit does: nothing (already saving), an error toast with nothing sent, or the payment. */
export type PayStep = { do: 'ignore' } | { do: 'refuse'; error: MessageKey } | { do: 'send'; request: Payment }

const REFUSAL: Record<Extract<PayCheck, { ok: false }>['reason'], MessageKey> = {
  from: 'stashes.accountError',
  amount: 'tx.errorAmount',
  interest: 'loans.interestError',
  overAmount: 'loans.interestOver',
  category: 'tx.chooseCategory',
}

/**
 * What a submit does. Nothing while a request is out; an error toast for a field that is wrong (the fields stay); else the payment. The interest and
 * its category go only when the interest is above 0, the note only when there is one.
 */
export function paySubmit(state: DialogState, liabilityId: string, fields: PayFields): PayStep {
  if (state !== 'EDITING') return { do: 'ignore' }
  const check = checkPay(fields)
  if (!check.ok) return { do: 'refuse', error: REFUSAL[check.reason] }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(fields.date)) return { do: 'refuse', error: 'common.invalidDate' }
  const note = fields.note.trim()
  if ([...note].length > PAY_NOTE_MAX) return { do: 'refuse', error: 'stashes.noteError' }
  return {
    do: 'send',
    request: {
      liabilityId,
      fromAccountId: fields.from,
      amount: fromMinor(check.amount),
      ...(check.interest > 0n ? { interest: fromMinor(check.interest), categoryId: fields.category } : {}),
      date: fields.date,
      ...(note ? { note } : {}),
    },
  }
}
