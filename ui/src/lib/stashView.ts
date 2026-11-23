import { isLiability, type Account } from '../api/accounts'
import type { Category } from '../api/categories'
import type { FactItem } from '../components/Facts'
import type { DepositState, Entry, EntryKind, EntryPatch, NewEntry, NewTake, ReturnInput, Stash } from '../api/stashes'
import type { MessageKey } from '../i18n/en'
import type { DialogState } from './accountDialogs'
import { currencyKey } from './accountsView'
import { memoByKey } from './cache'
import { amountString, formatDate, fromMinor, moneyIn, toMinor } from './money'

// What the screens of the stashes decide, apart from drawing (docs/stash-design.md, sections 3, 5.3 and 13). The core checks every rule again;
// everything here only lets the dialogs say it before a request goes out. Money is whole minor units (bigint): no floating point touches it.
// Amounts come in and go out as the two-decimal text the app sends ("450.00", "-12.50").

/** Minor units of an amount; a text that is not an amount counts as 0 (the app's answers never are one, and a field is checked before it gets here). */
const minor = (text: string | null | undefined): bigint => (text == null ? 0n : (toMinor(text) ?? 0n))
const clamp = (value: bigint, low: bigint, high: bigint): bigint => (value < low ? low : value > high ? high : value)
const atLeast0 = (value: bigint): bigint => (value > 0n ? value : 0n)
/** `a / b` as a number from 0 to 1, for the width of a bar. `b` must be above 0. */
const fraction = (a: bigint, b: bigint): number => Number(clamp((a * 10000n) / b, 0n, 10000n)) / 10000

/** An amount of a stash, in the stash's own currency (its account's; the app's for an account in the app's currency). */
export function stashMoney(stash: Pick<Stash, 'currency'>, locale: string): (amount: string) => string {
  return moneyIn(stash.currency, locale)
}

// -- the delta rule (section 3.1) ----------------------------------------------------------------------------------------------------

/** `ok`, or the one reason a change is refused, with the numbers the sentence needs. */
export type DeltaCheck =
  | { ok: true }
  /** Money in is more than the account has free. `free` is what there is, never below 0. */
  | { ok: false; reason: 'overFree'; free: string }
  /** Money in would take the stash past its target by `over`. */
  | { ok: false; reason: 'overTarget'; over: string }
  /** Money out is more than is in the stash. `saved` is what there is, never below 0. */
  | { ok: false; reason: 'overSaved'; saved: string }
  /** The stash takes no money in: it is archived, or so is its account (section 5.1), or it is a deposit that does not run. Only `checkEntry` says this one. */
  | { ok: false; reason: 'closed' }
  /** A deposit takes money in only while it runs (section 5.3). */
  | { ok: false; reason: 'notRunning' }
  /** A deposit that runs is emptied by breaking it, not by changing an entry (section 5.3). */
  | { ok: false; reason: 'running' }
  /** A deposit that is broken keeps its entries as they are; only a note changes (section 5.3). */
  | { ok: false; reason: 'broken' }

/**
 * The delta rule of section 3.1. `before` is what the stash holds, `change` what the write does to it (above 0 money goes in, below 0 it comes out),
 * `free` what the account has free before the write (counting the transfer that comes with an add, when there is one), `target` the stash's.
 * Money in must fit in what is free and keep the stash at or under its target; money out must leave 0 or more; no change is always allowed. When
 * money in breaks both limits, the reason is the one that allows less (the free money, or what is left to the target; at a tie the target).
 */
export function deltaCheck(before: string, change: string, free: string, target: string | null): DeltaCheck {
  const held = minor(before)
  const by = minor(change)
  const room = minor(free)
  if (by === 0n) return { ok: true }
  if (by < 0n) return held + by >= 0n ? { ok: true } : { ok: false, reason: 'overSaved', saved: fromMinor(atLeast0(held)) }
  const aim = target === null ? null : minor(target)
  const over = aim === null ? 0n : held + by - aim
  const tooMuchFree = by > room
  if (tooMuchFree && over > 0n) {
    // what each limit allows: free money, and what is left to the target (nothing once it is passed)
    const byFree = atLeast0(room)
    const byTarget = aim === null ? 0n : atLeast0(aim - held)
    return byFree < byTarget ? { ok: false, reason: 'overFree', free: fromMinor(byFree) } : { ok: false, reason: 'overTarget', over: fromMinor(over) }
  }
  if (tooMuchFree) return { ok: false, reason: 'overFree', free: fromMinor(atLeast0(room)) }
  if (over > 0n) return { ok: false, reason: 'overTarget', over: fromMinor(over) }
  return { ok: true }
}

/** What the money dialog knows when it checks an amount. */
export type EntryCheck = {
  kind: EntryKind
  /** What the stash holds now, and what its account has free (both as the stash says), and the stash's target. */
  saved: string
  free: string
  target: string | null
  /** What is typed: above 0. */
  amount: string
  /** For an entry being changed: the amount it has now. */
  old?: string
  /** An add that brings the money from another account: the transfer into this one makes room first, so the free money grows by the amount. */
  bring?: boolean
  /** The stash takes no money in (`canAdd` is false): an entry that would put more in is refused; one that takes out, or changes nothing, is not. */
  closed?: boolean
  /** The state of a deposit, left out for a goal: its own rules for money come before the delta rule (`depositRule`). */
  deposit?: DepositState | null
}

/** What an entry does to what the stash holds, signed: an add puts its amount in, a take takes it out. */
export function signedAmount(kind: EntryKind, amount: string): bigint {
  return kind === 'ADD' ? minor(amount) : -minor(amount)
}

/**
 * The delta rule for the money dialog: an add or a take of `amount`, or an entry being changed from `old` to `amount`. A change is checked by what it
 * does to the stash (the difference), as the core does: lowering an add or raising a take can be allowed when the account has no free money at all.
 * An add that brings its money from another account counts the transfer: the free money is what there is plus the amount, which is all there is room
 * for only while nothing is held above what the account has (with free below 0 an add is refused whatever comes in, as the core refuses it).
 */
export function checkEntry(input: EntryCheck): DeltaCheck {
  const by = signedAmount(input.kind, input.amount) - (input.old === undefined ? 0n : signedAmount(input.kind, input.old))
  if (input.closed && by > 0n) return { ok: false, reason: 'closed' }
  const deposit = depositRule(input.deposit ?? null, by, input.old !== undefined)
  if (!deposit.ok) return deposit
  const room = minor(input.free)
  const free = input.bring && input.kind === 'ADD' && input.old === undefined && room >= 0n ? room + minor(input.amount) : room
  return deltaCheck(input.saved, fromMinor(by), fromMinor(free), input.target)
}

/**
 * What a deposit lets happen to what it holds (section 5.3), before the delta rule: `by` is the change (above 0 money in, below 0 out), `editing` says
 * it comes from changing or deleting an entry, not from a new one. Money goes in only while it runs. A new take is allowed in every state (while it
 * runs it is the break, and the dialog fixes its amount to what is saved); an edit or a delete may not take money out of one that runs, and none
 * changes one that is broken. A goal (`state` null) and no change are always fine.
 */
export function depositRule(state: DepositState | null, by: bigint, editing: boolean): DeltaCheck {
  if (state === null || by === 0n) return { ok: true }
  if (state === 'BROKEN' && editing) return { ok: false, reason: 'broken' }
  if (by > 0n) return state === 'RUNNING' ? { ok: true } : { ok: false, reason: 'notRunning' }
  return editing && state === 'RUNNING' ? { ok: false, reason: 'running' } : { ok: true }
}

/**
 * Whether an entry can be deleted: deleting takes back what it did, so it is a change of minus its signed amount, checked by the rules of a deposit and
 * then by the delta rule (an add whose money was taken out since cannot go, a take only within what is free and under the target).
 */
export function checkRemoval(stash: Pick<Stash, 'saved' | 'free' | 'target' | 'deposit'>, entry: Pick<Entry, 'kind' | 'amount'>): DeltaCheck {
  const undone = -signedAmount(entry.kind, entry.amount)
  const own = depositRule(stash.deposit?.state ?? null, undone, true)
  return own.ok ? deltaCheck(stash.saved, fromMinor(undone), stash.free, stash.target) : own
}

/** What the stash would hold after the change, as an amount (it may be below 0 for a stash that already was). */
export function savedAfter(saved: string, change: bigint): string {
  return fromMinor(minor(saved) + change)
}

// -- the numbers of a bar ------------------------------------------------------------------------------------------------------------

/** `floor(saved x 100 / target)`: 0 for a stash with nothing in it, none without a target. It may be above 100. */
export function percentOf(saved: string, target: string | null): number | null {
  if (target === null) return null
  const held = minor(saved)
  const aim = minor(target)
  return held > 0n && aim > 0n ? Number((held * 100n) / aim) : 0
}

/** How full the bar of a stash is. */
export type Progress = {
  /** From 0 to 1: how much of the track is filled. A stash below 0 is empty and one over its target full; one with no target has no bar (0). */
  fill: number
  percent: number | null
  /** There is a target and the stash has reached it. */
  reached: boolean
  /** More than the target. */
  over: boolean
  /** Below 0 (a sync can leave a stash so). */
  below: boolean
}

export function progress(saved: string, target: string | null): Progress {
  const held = minor(saved)
  const aim = target === null ? null : minor(target)
  return {
    fill: aim !== null && aim > 0n ? fraction(held, aim) : 0,
    percent: percentOf(saved, target),
    reached: aim !== null && held >= aim,
    over: aim !== null && held > aim,
    below: held < 0n,
  }
}

/** The bar of the money dialog: what stays (solid) and what goes in or out (striped), as fractions of the track. */
export type PreviewBar = { solid: number; stripe: number; tone: 'add' | 'take' | 'same' }

/**
 * The bar before and after a change. With a target the track is the target; without one it is the larger of the two amounts, so the bar still
 * shows how much more or less there is. An add: the solid part is what is there, the stripe what goes in. A take: the solid part is what stays, the
 * stripe what goes out. A stash below 0 has nothing solid.
 */
export function previewBar(before: string, after: string, target: string | null): PreviewBar {
  const was = atLeast0(minor(before))
  const will = atLeast0(minor(after))
  const aim = target === null ? (was > will ? was : will) : minor(target)
  if (aim <= 0n) return { solid: 0, stripe: 0, tone: 'same' }
  const [a, b] = [fraction(was, aim), fraction(will, aim)]
  if (will > was) return { solid: a, stripe: b - a, tone: 'add' }
  if (will < was) return { solid: b, stripe: a - b, tone: 'take' }
  return { solid: a, stripe: 0, tone: 'same' }
}

// -- chips ----------------------------------------------------------------------------------------------------------------------------

/** One-tap amounts under the amount field: `id` says which words go with it. */
export type ChipId = 'rest' | 'month' | 'free' | 'all' | 'half'
export type Chip = { id: ChipId; amount: string }

/** The words of each chip, with `{amount}`. */
export const CHIP_WORDS: Record<ChipId, MessageKey> = {
  rest: 'stashes.chipRest',
  month: 'stashes.chipMonth',
  free: 'stashes.chipFree',
  all: 'stashes.chipAll',
  half: 'stashes.chipHalf',
}

/**
 * The amounts worth a tap, only those above 0, and each amount once (the first of two that agree stays). An add: the rest to the target, a month's
 * share (with a target date), and all the free money when that is less than what is left (or there is no target). A take: all that is saved, and
 * half of it rounded down to the cent.
 */
export function chips(stash: Pick<Stash, 'saved' | 'left' | 'perMonth' | 'free'>, kind: EntryKind): Chip[] {
  const found: Chip[] = []
  if (kind === 'ADD') {
    const left = stash.left === null ? null : minor(stash.left)
    const free = minor(stash.free)
    if (left !== null) found.push({ id: 'rest', amount: stash.left ?? '0.00' })
    if (stash.perMonth !== null) found.push({ id: 'month', amount: stash.perMonth })
    if (left === null || free < left) found.push({ id: 'free', amount: fromMinor(free) })
  } else {
    const saved = minor(stash.saved)
    found.push({ id: 'all', amount: fromMinor(saved) }, { id: 'half', amount: fromMinor(saved / 2n) })
  }
  const seen = new Set<string>()
  return found.filter((chip) => {
    if (minor(chip.amount) <= 0n || seen.has(chip.amount)) return false
    seen.add(chip.amount)
    return true
  })
}

// -- what a stash can do ----------------------------------------------------------------------------------------------------------------

/** Whether money can be put in: not into an archived stash, nor one whose account is archived (section 5.1), and into a deposit only while it runs (5.3). */
export function canAdd(stash: Pick<Stash, 'archived' | 'accountArchived' | 'deposit'>): boolean {
  return !stash.archived && !stash.accountArchived && (!stash.deposit || stash.deposit.state === 'RUNNING')
}

/** Whether money can be taken out: only when something is in it (a stash below 0 offers only an add), whatever the state of a deposit. */
export function canTake(stash: Pick<Stash, 'saved'>): boolean {
  return minor(stash.saved) > 0n
}

/** Whether taking money out of the stash breaks it: a deposit that runs (section 5.3). The take is then the whole of what it holds. */
export function isBreak(stash: Pick<Stash, 'deposit'>): boolean {
  return stash.deposit?.state === 'RUNNING'
}

/** Whether the return of a deposit can be booked: it has matured, its return is not booked, and neither it nor its account is archived (section 5.5). */
export function canBookReturn(stash: Pick<Stash, 'archived' | 'accountArchived' | 'deposit'>): boolean {
  return stash.deposit?.state === 'MATURED' && !stash.archived && !stash.accountArchived
}

/** A deposit that is booked or broken and holds nothing: there is nothing left to do with it but close it, which is archiving. */
export function canClose(stash: Pick<Stash, 'archived' | 'saved' | 'deposit'>): boolean {
  return !stash.archived && (stash.deposit?.state === 'BOOKED' || stash.deposit?.state === 'BROKEN') && minor(stash.saved) === 0n
}

/** What the stash takes away from its account: what it holds, never below 0 (section 3). */
export function held(stash: Pick<Stash, 'saved'>): bigint {
  return atLeast0(minor(stash.saved))
}

/** The stashes that are not archived, and the archived ones, each in the order they came in. */
export function activeStashes<T extends Pick<Stash, 'archived'>>(list: readonly T[]): T[] {
  return list.filter((stash) => !stash.archived)
}
export function archivedStashes<T extends Pick<Stash, 'archived'>>(list: readonly T[]): T[] {
  return list.filter((stash) => stash.archived)
}

/** Whether the stash holds more than its account has: a warning belongs under it (section 3, `free`). */
export function overBooked(free: string): boolean {
  return minor(free) < 0n
}

/** Whether an amount is not 0 ("0.00", "-0.00" are not). */
export function isSome(amount: string): boolean {
  return minor(amount) !== 0n
}

// -- the line under a card ----------------------------------------------------------------------------------------------------------------

/**
 * What is next for a stash, in words for the card and the page: it has no target (nothing to say), it has reached it, its date has passed, or this
 * much is left, with what to save each month until its date when it has one.
 */
export type NextLine =
  | { kind: 'reached' }
  | { kind: 'overdue'; since: string }
  | { kind: 'left'; left: string; perMonth: string | null; until: string | null }

export function nextLine(stash: Pick<Stash, 'target' | 'left' | 'reached' | 'overdue' | 'targetDate' | 'perMonth'>): NextLine | null {
  if (stash.target === null) return null
  if (stash.reached) return { kind: 'reached' }
  if (stash.overdue && stash.targetDate) return { kind: 'overdue', since: stash.targetDate }
  if (stash.left === null || minor(stash.left) <= 0n) return null
  return stash.perMonth !== null && stash.targetDate !== null
    ? { kind: 'left', left: stash.left, perMonth: stash.perMonth, until: stash.targetDate }
    : { kind: 'left', left: stash.left, perMonth: null, until: null }
}

const textFormatter = memoByKey(
  (locale: string, kind: string) => `${locale}|${kind}`,
  (locale: string, kind: string) => {
    try {
      return new Intl.DateTimeFormat(locale, kind === 'month' ? { month: 'long', year: 'numeric', timeZone: 'UTC' } : { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' })
    } catch {
      return null
    }
  },
)

function written(isoDate: string, locale: string, kind: 'month' | 'day'): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate)
  if (!match) return isoDate
  const text = textFormatter(locale, kind)?.format(new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]))))
  if (text === undefined) return isoDate
  try {
    return text.toLocaleLowerCase(locale)
  } catch {
    return text.toLowerCase()
  }
}

/** "june 2027": the month of a date in words, lower case in the person's language, for "a month until ...". */
export function monthWords(isoDate: string, locale: string): string {
  return written(isoDate, locale, 'month')
}

/** "30 june 2026": a date in words, lower case in the person's language, for "overdue since ...". */
export function dayWords(isoDate: string, locale: string): string {
  return written(isoDate, locale, 'day')
}

type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string

/**
 * "450.00 of 1200.00 · 37%" for a stash with a target, "450.00 saved" for one without. `money` writes an amount in the stash's currency. The
 * percent is the app's own (it may be above 100).
 */
export function figures(stash: Pick<Stash, 'saved' | 'target' | 'percent'>, money: (amount: string) => string, t: Translate): string {
  if (stash.target === null) return t('stashes.saved', { amount: money(stash.saved) })
  const of = t('stashes.of', { saved: money(stash.saved), target: money(stash.target) })
  return stash.percent === null ? of : `${of} · ${t('stashes.percent', { percent: stash.percent })}`
}

// -- a row of the list (docs/loans-deposits-design.md, section 10) ---------------------------------------------------------------------

/** How a word of a row reads: plain, quiet (nothing to do), good news (reached, matured) or a warning (overdue, broken). */
export type Tone = 'plain' | 'quiet' | 'good' | 'late'

/** The progress column of a row, and the bar line of a phone row: how full the bar is, whether it is full in the success colour, and the short figure beside it. */
export type RowMeter = { fill: number; reached: boolean; label: string }

const dayNumber = (isoDate: string): number | null => {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate)
  return match ? Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])) / 86_400_000 : null
}

/**
 * The bar of a row. A goal with a target: what is saved of it, and its percent. A deposit that runs: how far through its term it is (start to
 * maturity), and the days left; one that matured: full, with no figure (its state says it). None for a goal without a target, or a deposit booked or broken.
 */
export function rowMeter(stash: Pick<Stash, 'saved' | 'target' | 'targetDate' | 'deposit'>, t: Translate): RowMeter | null {
  const deposit = stash.deposit
  if (deposit) {
    if (deposit.state === 'MATURED') return { fill: 1, reached: true, label: '' }
    if (deposit.state !== 'RUNNING') return null
    const start = dayNumber(deposit.startDate)
    const end = stash.targetDate === null ? null : dayNumber(stash.targetDate)
    const term = start === null || end === null ? 0 : end - start
    const fill = term > 0 ? Math.min(1, Math.max(0, (term - deposit.daysLeft) / term)) : 1
    return { fill, reached: false, label: t('stashes.daysShort', { days: deposit.daysLeft }) }
  }
  if (stash.target === null) return null
  const shown = progress(stash.saved, stash.target)
  return { fill: shown.fill, reached: shown.reached, label: t('stashes.percent', { percent: shown.percent ?? 0 }) }
}

/** What a row says of where a stash stands, with its tone. */
export type RowStatus = { text: string; tone: Tone }

/**
 * The status column of a row. A deposit: its state (matured is good news, broken a warning, booked quiet). A goal: reached, overdue, what to save each month
 * until its date, what is left without a date, or "—" when it has no target. `money` writes an amount in the stash's currency.
 */
export function rowStatus(stash: Pick<Stash, 'target' | 'left' | 'reached' | 'overdue' | 'perMonth' | 'deposit'>, money: (amount: string) => string, t: Translate): RowStatus {
  if (stash.deposit) {
    const state = stash.deposit.state
    return { text: t(`stashes.state.${state}`), tone: state === 'MATURED' ? 'good' : state === 'BROKEN' ? 'late' : state === 'BOOKED' ? 'quiet' : 'plain' }
  }
  if (stash.target === null) return { text: '—', tone: 'quiet' }
  if (stash.reached) return { text: t('stashes.reached'), tone: 'good' }
  if (stash.overdue) return { text: t('stashes.overdueWord'), tone: 'late' }
  if (stash.perMonth !== null) return { text: t('stashes.monthly', { amount: money(stash.perMonth) }), tone: 'plain' }
  if (stash.left !== null && minor(stash.left) > 0n) return { text: t('stashes.left', { amount: money(stash.left) }), tone: 'plain' }
  return { text: '—', tone: 'quiet' }
}

/** The kinds the list can be narrowed to: '' is all of them. */
export const KIND_FILTERS = ['', 'GOAL', 'DEPOSIT'] as const
export type KindFilter = (typeof KIND_FILTERS)[number]

/**
 * The total row under the table: what the shown stashes hold, and the targets of those that have one. Only for two or more, all in one currency (a sum of
 * amounts in different currencies means nothing); the total saved over the list is the one in the app's currency.
 */
export function listTotal(list: readonly Pick<Stash, 'saved' | 'target' | 'currency'>[]): { saved: string; target: string | null; currency: Stash['currency'] } | null {
  if (list.length < 2) return null
  const currency = list[0].currency
  if (list.some((stash) => stash.currency !== currency)) return null
  const targets = list.filter((stash) => stash.target !== null)
  return {
    saved: fromMinor(list.reduce((sum, stash) => sum + minor(stash.saved), 0n)),
    target: targets.length === 0 ? null : fromMinor(targets.reduce((sum, stash) => sum + minor(stash.target), 0n)),
    currency,
  }
}

/**
 * The mark after the name of a phone row: only a state that matters (reached, matured: good news; overdue, broken: a warning). The other states of a
 * row (a deposit that runs, one that is booked) are the grey line under the name, and a goal that is on its way has its bar there.
 */
export function rowMark(status: RowStatus): RowStatus | null {
  return status.tone === 'good' || status.tone === 'late' ? status : null
}

/**
 * What a stash says of itself, one fact to a line (docs/ui-design.md, rule 4), on its page and in the open row of a phone: where it lives (`account`, the
 * caller's own node: a link on the page), its target and how far it is, what is left and what to save each month, its date (a goal's target date, a
 * deposit's maturity), where a goal stands when that is news, then a deposit's rate, start, days left, the return it expects, its value at maturity and
 * its state (with the day it was closed). What it holds is not here: the row and the header of the page have that. `Facts` leaves out a pair with no value.
 */
export function stashFacts(stash: Stash, account: FactItem['value'], money: (amount: string) => string, locale: string, t: Translate): FactItem[] {
  const deposit = stash.deposit
  const live = deposit?.state === 'RUNNING' || deposit?.state === 'MATURED'
  const closedOn = deposit ? (deposit.brokenOn ?? (deposit.state === 'BOOKED' ? deposit.returnedOn : null)) : null
  return [
    { label: t('stashes.account'), value: account },
    { label: t('stashes.target'), value: stash.target === null ? null : money(stash.target) },
    { label: t('stashes.progress'), value: stash.percent === null ? null : t('stashes.percent', { percent: stash.percent }), tone: stash.reached ? 'positive' : undefined },
    { label: t('stashes.leftLabel'), value: stash.left !== null && minor(stash.left) > 0n ? money(stash.left) : null },
    { label: t('stashes.perMonthLabel'), value: stash.perMonth === null ? null : money(stash.perMonth) },
    { label: deposit ? t('stashes.maturity') : t('stashes.targetDate'), value: stash.targetDate === null ? null : formatDate(stash.targetDate, locale), tone: stash.overdue ? 'negative' : undefined },
    { label: t('stashes.status'), value: deposit ? null : stash.reached ? t('stashes.reached') : stash.overdue ? t('stashes.overdueWord') : null, tone: stash.reached ? 'positive' : 'negative' },
    { label: t('accounts.rate'), value: deposit ? t('accounts.rateShown', { rate: deposit.interestRate, period: t(`accounts.period.${deposit.interestPeriod}`) }) : null },
    { label: t('stashes.startDate'), value: deposit ? formatDate(deposit.startDate, locale) : null },
    { label: t('stashes.daysLeft'), value: deposit?.state === 'RUNNING' ? String(deposit.daysLeft) : null },
    { label: t('stashes.expectedReturn'), value: deposit && live ? money(deposit.expectedReturn) : null },
    { label: t('stashes.valueAtMaturity'), value: deposit && live ? money(deposit.valueAtMaturity) : null },
    { label: t('stashes.depositState'), value: deposit ? t(`stashes.state.${deposit.state}`) : null, tone: deposit?.state === 'MATURED' ? 'positive' : deposit?.state === 'BROKEN' ? 'negative' : undefined },
    { label: t('stashes.closedOn'), value: closedOn === null ? null : formatDate(closedOn, locale) },
  ]
}

/** The words of `nextLine`: "750.00 left · 107.15 a month until june 2027", "750.00 left", "reached", "overdue since 30 june 2026" (a warning). */
export function nextText(line: NextLine, money: (amount: string) => string, t: Translate, locale: string): { text: string; warning: boolean } {
  if (line.kind === 'reached') return { text: t('stashes.reached'), warning: false }
  if (line.kind === 'overdue') return { text: t('stashes.overdue', { date: dayWords(line.since, locale) }), warning: true }
  const left = t('stashes.left', { amount: money(line.left) })
  return {
    text: line.perMonth !== null && line.until !== null ? `${left} · ${t('stashes.perMonth', { amount: money(line.perMonth), date: monthWords(line.until, locale) })}` : left,
    warning: false,
  }
}

// -- the accounts a stash may use ---------------------------------------------------------------------------------------------------------

/**
 * The accounts money can come from or go to with an entry of this stash: the other assets that are not archived and have the stash's currency
 * (an account compares by `currencyKey`: the app's own currency, or a code). None when the stash's account is unknown.
 */
export function otherAccounts(accounts: readonly Account[], stash: Pick<Stash, 'accountId'>): Account[] {
  const own = accounts.find((account) => account.id === stash.accountId)
  if (!own) return []
  return accounts.filter((account) => !account.archived && !isLiability(account.kind) && account.id !== own.id && currencyKey(account) === currencyKey(own))
}

/**
 * The accounts a stash may live in: the assets that are not archived (a credit, a loan or a mortgage holds no stash). Moving a stash keeps its currency, so for a stash that is made already only the
 * accounts in the currency of its own are offered, and its own account stays in the list even when it is archived. `hidden` says that some account
 * was left out for its currency (the line "only accounts in ...").
 */
export function homeAccounts(accounts: readonly Account[], stash: Pick<Stash, 'accountId'> | null): { choices: Account[]; hidden: boolean } {
  const active = accounts.filter((account) => !account.archived && !isLiability(account.kind))
  const own = stash ? accounts.find((account) => account.id === stash.accountId) : undefined
  if (!stash || !own) return { choices: active, hidden: false }
  const choices = accounts.filter((account) => account.id === own.id || (!account.archived && !isLiability(account.kind) && currencyKey(account) === currencyKey(own)))
  return { choices, hidden: active.some((account) => !choices.includes(account)) }
}

/**
 * Whether a stash that holds `saved` can move to `account` as far as the money goes: what it holds must fit in what is free there (the account does
 * not yet count the stash, section 5.1). The same account is not a move.
 */
export function fitsIn(stash: Pick<Stash, 'accountId' | 'saved'>, account: Pick<Account, 'id' | 'free'>): boolean {
  return account.id === stash.accountId || held(stash) === 0n || held(stash) <= minor(account.free)
}

// -- the money dialog ---------------------------------------------------------------------------------------------------------------------

/** The most characters of the note of an entry (section 5.2), of the notes of a stash, and of the name. */
export const ENTRY_NOTE_MAX = 200
export const STASH_NOTES_MAX = 1000

/** The fields of the money dialog. `account` is the account the money comes from (an add) or goes to (a take) while `transfer` is on. */
export type MoneyFields = { kind: EntryKind; amount: string; date: string; note: string; transfer: boolean; account: string }

/** A fresh dialog: no amount, no note, today, no transfer. */
export function moneyStart(kind: EntryKind, today: string): MoneyFields {
  return { kind, amount: '', date: today, note: '', transfer: false, account: '' }
}

/** The dialog of an entry being changed: what the entry has. */
export function moneyOfEntry(entry: Entry): MoneyFields {
  return { kind: entry.kind, amount: entry.amount, date: entry.date, note: entry.note ?? '', transfer: false, account: '' }
}

/**
 * Another tab: the amount, the date and the note stay, the transfer choice goes (section 5.3). The account is kept to start the next choice from,
 * but off. For a deposit that runs (`whole` is what it holds) the take tab is the break, whose amount is the whole of it: it is put there on
 * the way in and taken out again on the way back.
 */
export function moneyTab(fields: MoneyFields, kind: EntryKind, whole?: string): MoneyFields {
  const next: MoneyFields = { ...fields, kind, transfer: false, account: '' }
  if (whole === undefined || kind === fields.kind) return next
  return kind === 'TAKE' ? { ...next, amount: whole } : { ...next, amount: '' }
}

/** The date asked for, but never after today (nothing is put in or taken out in the future, section 16 choice 7). */
export function notAfter(date: string, today: string): string {
  return date > today ? today : date
}

/** What the money dialog's submit decides. `edit` is the entry being changed. */
export type MoneyStep =
  | { do: 'ignore' }
  | { do: 'refuse'; error: MessageKey }
  | { do: 'close' }
  | { do: 'send'; request: { add: NewEntry } | { take: NewTake } | { patch: EntryPatch } }

/**
 * What a submit does. Nothing while a request is out, or while the amount breaks the rule (the dialog says why and its button is off). An error toast
 * for an amount that is no amount, a date after today, or a note that is too long; the fields stay. An entry being changed sends only what is
 * different, and nothing different just closes. A new one sends the amount, the date, the note when there is one, and the other account when the
 * transfer is on.
 */
export function moneySubmit(
  state: DialogState,
  edit: Entry | null,
  fields: MoneyFields,
  today: string,
  rule: DeltaCheck,
): MoneyStep {
  if (state !== 'EDITING') return { do: 'ignore' }
  const amount = amountString(fields.amount, false)
  if (amount === null) return { do: 'refuse', error: 'tx.errorAmount' }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(fields.date) || fields.date > today) return { do: 'refuse', error: 'stashes.dateError' }
  const note = fields.note.trim()
  if ([...note].length > ENTRY_NOTE_MAX) return { do: 'refuse', error: 'stashes.noteError' }
  if (!rule.ok) return { do: 'ignore' }
  if (edit) {
    const patch: EntryPatch = {}
    if (amount !== edit.amount) patch.amount = amount
    if (fields.date !== edit.date) patch.date = fields.date
    if (note !== (edit.note ?? '')) patch.note = note === '' ? null : note
    return Object.keys(patch).length === 0 ? { do: 'close' } : { do: 'send', request: { patch } }
  }
  const other = fields.transfer && fields.account !== '' ? fields.account : undefined
  return fields.kind === 'ADD'
    ? { do: 'send', request: { add: { amount, date: fields.date, ...(note ? { note } : {}), ...(other ? { fromAccountId: other } : {}) } } }
    : { do: 'send', request: { take: { amount, date: fields.date, ...(note ? { note } : {}), ...(other ? { toAccountId: other } : {}) } } }
}

// -- the return of a deposit (section 5.5) ---------------------------------------------------------------------------------------------------

/** The money dialog's three modes: an add, a take, and the return of a deposit that matured. */
export type MoneyMode = EntryKind | 'RETURN'

/** The fields of the return mode. `category` is an income category ('' for none yet). */
export type ReturnFields = { amount: string; category: string; date: string; note: string }

/**
 * The return dialog as it opens: the amount the deposit expects to return (empty when that is nothing), the category used last time while it is still an
 * income category, today, no note.
 */
export function returnStart(stash: Pick<Stash, 'deposit'>, categories: readonly Pick<Category, 'id' | 'type'>[], last: string, today: string): ReturnFields {
  const expected = stash.deposit?.expectedReturn ?? ''
  return {
    amount: minor(expected) > 0n ? expected : '',
    category: categories.find((category) => category.id === last && category.type === 'INCOME')?.id ?? '',
    date: today,
    note: '',
  }
}

/** The date of the return held between the maturity and today: the core takes no other (an income is not made in the future, nor before the term ended). */
export function returnDate(date: string, maturity: string, today: string): string {
  return date > today ? today : date < maturity ? maturity : date
}

/** What stops the return from being booked, in the order the fields come; null when nothing does. Done is off while this is not null. */
export function returnStop(fields: ReturnFields, maturity: string | null, today: string): 'amount' | 'category' | 'date' | 'note' | null {
  if (amountString(fields.amount, false) === null) return 'amount'
  if (fields.category === '') return 'category'
  if (!/^\d{4}-\d{2}-\d{2}$/.test(fields.date) || fields.date > today || (maturity !== null && fields.date < maturity)) return 'date'
  if ([...fields.note.trim()].length > ENTRY_NOTE_MAX) return 'note'
  return null
}

const RETURN_REFUSAL = { amount: 'tx.errorAmount', category: 'tx.chooseCategory', date: 'stashes.returnDateError', note: 'stashes.noteError' } as const

/** What the submit of the return does: nothing while a request is out, an error toast with nothing sent, or the return as the core takes it. */
export type ReturnStep = { do: 'ignore' } | { do: 'refuse'; error: MessageKey } | { do: 'send'; request: ReturnInput }

export function returnSubmit(state: DialogState, stash: Pick<Stash, 'targetDate'>, fields: ReturnFields, today: string): ReturnStep {
  if (state !== 'EDITING') return { do: 'ignore' }
  const stop = returnStop(fields, stash.targetDate, today)
  const amount = amountString(fields.amount, false)
  if (stop !== null || amount === null) return { do: 'refuse', error: RETURN_REFUSAL[stop ?? 'amount'] }
  const note = fields.note.trim()
  return { do: 'send', request: { amount, categoryId: fields.category, date: fields.date, ...(note ? { note } : {}) } }
}

/** The tab an `Alt` + digit key press chooses in the money dialog: 1 is add, 2 is take out. The key's code is used (a Mac's `Option+1` types another character). */
export function kindForKey(event: { code: string; altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }): EntryKind | null {
  if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return null
  return event.code === 'Digit1' ? 'ADD' : event.code === 'Digit2' ? 'TAKE' : null
}

/**
 * Whether an add that this entry makes has brought the stash to its target: the target is reached after, and was not before. The toast of
 * `stashes.reachedToast` is for that moment.
 */
export function reachedNow(before: Pick<Stash, 'reached'>, after: Pick<Stash, 'reached'>): boolean {
  return !before.reached && after.reached
}
