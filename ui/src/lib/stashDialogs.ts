import type { Account } from '../api/accounts'
import type { InterestPeriod } from '../api/accounts'
import { cleanCategoryName } from '../api/categories'
import type { NewStash, Stash, StashKind, StashPatch } from '../api/stashes'
import type { MessageKey } from '../i18n/en'
import { cleanPercent, type DialogState } from './accountDialogs'
import { accountById, defaultAccountId, listable } from './accountsView'
import { amountString, toMinor } from './money'
import { fitsIn, STASH_NOTES_MAX } from './stashView'

// The dialog that makes a stash or changes one (docs/stash-design.md, sections 5.3 and 13.5; docs/loans-deposits-design.md, sections 5.3 and 5.6): its
// fields, what stops it, and what a submit sends. The core checks every rule again; this only says it before a request goes out.

/**
 * The fields of the dialog, as typed. `targetDate` is '' for none; for a deposit it is the maturity, which is never none. `kind` is the switch of a new
 * stash (a stash that is made keeps its kind). `rate` is a percent as typed, with its `period`, and `startDate` the day a deposit's term begins: a goal
 * leaves all three alone.
 */
export type StashFields = {
  name: string
  account: string
  target: string
  targetDate: string
  notes: string
  kind: StashKind
  rate: string
  period: InterestPeriod
  startDate: string
}

/**
 * The fields a dialog starts with: the stash's own for one being changed; else empty, in the account asked for (the page it was opened from)
 * if that account is there and not archived, else the account a new row starts with, a goal that starts today per year when it is made a deposit.
 */
export function stashFields(stash: Stash | null, accounts: readonly Account[], accountId?: string, today = ''): StashFields {
  if (stash) {
    const deposit = stash.deposit
    return {
      name: stash.name,
      account: stash.accountId,
      target: stash.target ?? '',
      targetDate: stash.targetDate ?? '',
      notes: stash.notes ?? '',
      kind: stash.kind,
      rate: deposit?.interestRate ?? '',
      period: deposit?.interestPeriod ?? 'YEARLY',
      startDate: deposit?.startDate ?? '',
    }
  }
  const asked = accountId !== undefined && listable(accounts).some((account) => account.id === accountId) ? accountId : defaultAccountId(accounts)
  return { name: '', account: asked, target: '', targetDate: '', notes: '', kind: 'GOAL', rate: '', period: 'YEARLY', startDate: today }
}

/** Whether the rate, the period, the start date and the maturity of a stash can be changed: a deposit that is broken keeps them (section 5.3). */
export function termIsFixed(stash: Stash | null): boolean {
  return stash?.deposit?.state === 'BROKEN'
}

/** The switch of a new stash to a deposit: a maturity is needed, so one starts a year on from the start date unless a date is there already. */
export function withKind(fields: StashFields, kind: StashKind): StashFields {
  if (kind === 'DEPOSIT' && fields.targetDate === '' && fields.startDate !== '') return { ...fields, kind, targetDate: monthsOn(fields.startDate, 12) }
  return { ...fields, kind }
}

/** What stops a stash from being saved as it is typed: a target below what is saved, or a move to an account that cannot hold what the stash holds. */
export type StashBlock =
  | { reason: 'targetBelowSaved'; saved: string }
  | { reason: 'moveLacks'; account: Account; free: string; saved: string }

/**
 * Only for a stash being changed (section 5.1). A target that is new and below what is saved is refused, clearing it never is (and neither is the
 * target it has). A new account must have enough free for what the stash holds (a stash below 0 holds nothing). Null when nothing stops it.
 */
export function stashBlock(stash: Stash | null, fields: StashFields, accounts: readonly Account[]): StashBlock | null {
  if (!stash) return null
  const target = amountString(fields.target, false)
  const aim = target === null ? null : toMinor(target)
  const saved = toMinor(stash.saved)
  if (target !== null && target !== stash.target && aim !== null && saved !== null && aim < saved) return { reason: 'targetBelowSaved', saved: stash.saved }
  const home = accountById(accounts, fields.account)
  if (home && fields.account !== stash.accountId && !fitsIn(stash, home)) {
    return { reason: 'moveLacks', account: home, free: home.free, saved: stash.saved }
  }
  return null
}

/** What a submit asks for: a new stash, or the fields of the one being changed that are different. */
export type StashRequest = { create: NewStash } | { patch: StashPatch }

export type StashStep = { do: 'ignore' } | { do: 'refuse'; error: MessageKey } | { do: 'close' } | { do: 'send'; request: StashRequest }

/**
 * What a submit does. Nothing while a request is out or while the dialog is stopped (`block`). An error toast for a name, target, date, notes or
 * account that is wrong; the fields stay. A stash being changed sends only what is different (`null` clears the target, the date and the notes), and
 * nothing different just closes.
 */
export function stashSubmit(state: DialogState, stash: Stash | null, fields: StashFields, block: StashBlock | null): StashStep {
  if (state !== 'EDITING') return { do: 'ignore' }
  const name = cleanCategoryName(fields.name)
  if (name === null) return { do: 'refuse', error: 'stashes.nameError' }
  if (fields.account === '') return { do: 'refuse', error: 'stashes.accountError' }
  const typedTarget = fields.target.trim()
  const target = typedTarget === '' ? null : amountString(typedTarget, false)
  if (typedTarget !== '' && target === null) return { do: 'refuse', error: 'stashes.targetError' }
  if (fields.targetDate !== '' && !/^\d{4}-\d{2}-\d{2}$/.test(fields.targetDate)) return { do: 'refuse', error: 'common.invalidDate' }
  const notes = fields.notes.trim()
  if ([...notes].length > STASH_NOTES_MAX) return { do: 'refuse', error: 'stashes.notesError' }
  // a deposit's rate, start and maturity, unless it is broken (it keeps them, and they are not sent)
  const deposit = (stash ? stash.kind : fields.kind) === 'DEPOSIT' && !termIsFixed(stash)
  const rate = cleanPercent(fields.rate)
  if (deposit) {
    if (rate === null) return { do: 'refuse', error: 'accounts.interestError' }
    if (!/^\d{4}-\d{2}-\d{2}$/.test(fields.startDate)) return { do: 'refuse', error: 'common.invalidDate' }
    if (fields.targetDate <= fields.startDate) return { do: 'refuse', error: 'stashes.maturityError' }
  }
  if (block) return { do: 'ignore' }
  if (!stash) {
    return {
      do: 'send',
      request: {
        create: {
          name,
          accountId: fields.account,
          ...(deposit && rate !== null ? { kind: 'DEPOSIT' as const, interestRate: rate, interestPeriod: fields.period, startDate: fields.startDate } : {}),
          ...(target !== null ? { target } : {}),
          ...(fields.targetDate !== '' ? { targetDate: fields.targetDate } : {}),
          ...(notes !== '' ? { notes } : {}),
        },
      },
    }
  }
  const patch: StashPatch = {}
  if (name !== stash.name) patch.name = name
  if (fields.account !== stash.accountId) patch.accountId = fields.account
  if (target !== stash.target) patch.target = target
  const date = fields.targetDate === '' ? null : fields.targetDate
  if (date !== stash.targetDate) patch.targetDate = date
  if ((notes === '' ? null : notes) !== stash.notes) patch.notes = notes === '' ? null : notes
  if (deposit && stash.deposit && rate !== null) {
    if (rate !== stash.deposit.interestRate) patch.interestRate = rate
    if (fields.period !== stash.deposit.interestPeriod) patch.interestPeriod = fields.period
    if (fields.startDate !== stash.deposit.startDate) patch.startDate = fields.startDate
  }
  return Object.keys(patch).length === 0 ? { do: 'close' } : { do: 'send', request: { patch } }
}

/** A date `count` months on from `date` ("YYYY-MM-DD"), held to the last day of that month (the 31st of January plus one month is the 28th or 29th of February). */
export function monthsOn(date: string, count: number): string {
  const [year, month, day] = date.split('-').map(Number)
  const index = year * 12 + (month - 1) + count
  const nextYear = Math.floor(index / 12)
  const nextMonth = (index % 12) + 1
  const last = new Date(Date.UTC(nextYear, nextMonth, 0)).getUTCDate()
  return `${nextYear}-${String(nextMonth).padStart(2, '0')}-${String(Math.min(day, last)).padStart(2, '0')}`
}

/** A date a month on from `date`: where a target date starts when the person chooses to set one. */
export function aMonthOn(date: string): string {
  return monthsOn(date, 1)
}
