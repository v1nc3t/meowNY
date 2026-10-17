import { EXPORT_SECTIONS, type ExportSection, type ExportSelection } from '../api/dataExport'
import { apiBound, shiftBound, thisPeriod, type RangeMode } from '../lib/range'

export type ExportFormat = 'json' | 'csv' | 'xlsx' | 'pdf'

export const FORMATS: readonly ExportFormat[] = ['json', 'csv', 'xlsx', 'pdf']

/** A report covers at most this many months (the app refuses more). */
export const MAX_REPORT_MONTHS = 120

/** What the person chose on the export screen, apart from the format. */
export type Choice = {
  /** False means every date. */
  limitDates: boolean
  /** How the range is chosen, as on the transactions page: by day, by month or by year. */
  mode: RangeMode
  /** In the units of `mode`: "YYYY-MM-DD", "YYYY-MM" or "YYYY". Only used when `limitDates` is on. */
  from: string
  to: string
  /** False means only `categoryIds`. */
  allCategories: boolean
  categoryIds: string[]
  /** Income or expenses only; empty means both. */
  kind: '' | 'INCOME' | 'EXPENSE'
  /** What a transaction cost you, at least and at most, as typed. Empty means no bound. */
  minAmount: string
  maxAmount: string
  /** Only transactions with this text in the name or the description, as typed. */
  text: string
  /** Only the bills shared with friends, or only those that are not; empty means both. */
  split: '' | 'split' | 'not_split'
  /** The parts of a JSON or Excel file that go in. */
  sections: ExportSection[]
}

/** The longest text a transaction may be looked for by (the app refuses more). */
export const TEXT_MAX_CHARS = 100

/** The parts that have a sheet in an Excel file: everything but the change history. */
const SHEETED: readonly ExportSection[] = EXPORT_SECTIONS.filter((section) => section !== 'history')

export type Span = { firstDate: string | null; lastDate: string | null }

export type Problem = 'dates' | 'categories' | 'months' | 'amount' | 'sections'

/** Every date and every category, and a range of the current month ready for when the person limits the dates. */
export function defaultChoice(today: { date: string; month: string }): Choice {
  return {
    limitDates: false,
    mode: 'month',
    from: thisPeriod('month', today),
    to: thisPeriod('month', today),
    allCategories: true,
    categoryIds: [],
    kind: '',
    minAmount: '',
    maxAmount: '',
    text: '',
    split: '',
    sections: [...EXPORT_SECTIONS],
  }
}

/**
 * An amount as the app takes it: "12", "12.5" and "12,50" are "12.00", "12.50" and "12.50". Empty is empty (no bound); anything else, or
 * more than ten whole digits, is null.
 */
export function amountText(text: string): string | null {
  const typed = text.trim()
  if (typed === '') return ''
  const match = /^(\d{1,10})(?:[.,](\d{1,2}))?$/.exec(typed)
  if (!match) return null
  return `${(match[1] ?? '0').replace(/^0+(?=\d)/, '')}.${(match[2] ?? '').padEnd(2, '0')}`
}

/** Whether anything but the parts of the file is limited: dates, categories, income or expenses, an amount or a text. */
export function isLimited(choice: Choice): boolean {
  return choice.limitDates || !choice.allCategories || choice.kind !== '' || choice.minAmount.trim() !== '' || choice.maxAmount.trim() !== '' || choice.text.trim() !== '' || choice.split !== ''
}

/** The parts that go into a file of this format: for JSON all but a history that a limit would leave out, for Excel those with a sheet. */
export function sectionsFor(format: ExportFormat, choice: Choice): ExportSection[] {
  if (format === 'json') return choice.sections.filter((section) => section !== 'history' || !isLimited(choice))
  if (format === 'xlsx') return choice.sections.filter((section) => SHEETED.includes(section))
  return choice.sections
}

/**
 * The ways a range can be chosen for a format. A PDF report is made of whole months (its budgets, its chart and its comparison are
 * monthly), so it has no choice by day: asking for one would only be widened to the months, and quietly.
 */
export function modesFor(format: ExportFormat): readonly RangeMode[] {
  return format === 'pdf' ? ['month', 'year'] : ['date', 'month', 'year']
}

/** The choice with its range chosen in a way the format allows: a range by day becomes the months it touches. */
export function fitFormat(format: ExportFormat, choice: Choice): Choice {
  if (modesFor(format).includes(choice.mode)) return choice
  return {
    ...choice,
    mode: 'month',
    from: shiftBound(choice.from, choice.mode, 'month', 'from'),
    to: shiftBound(choice.to, choice.mode, 'month', 'to'),
  }
}

/** The same choice, chosen by another unit. */
export function withMode(choice: Choice, mode: RangeMode): Choice {
  return { ...choice, mode, from: shiftBound(choice.from, choice.mode, mode, 'from'), to: shiftBound(choice.to, choice.mode, mode, 'to') }
}

/** The first and last day the range covers, "YYYY-MM-DD", or empty for an end that is not a whole value of its unit. */
export function bounds(choice: Choice): { from: string; to: string } {
  return { from: apiBound(choice.mode, choice.from, 'from'), to: apiBound(choice.mode, choice.to, 'to') }
}

/** The last day of "YYYY-MM" as "YYYY-MM-DD". */
export function monthEnd(month: string): string {
  const [year, number] = month.split('-').map(Number)
  return `${month}-${String(new Date(Date.UTC(year, number, 0)).getUTCDate()).padStart(2, '0')}`
}

/** How many months from `fromMonth` to `toMonth`, both included. */
export function monthsBetween(fromMonth: string, toMonth: string): number {
  const [fy, fm] = fromMonth.split('-').map(Number)
  const [ty, tm] = toMonth.split('-').map(Number)
  return (ty - fy) * 12 + (tm - fm) + 1
}

/**
 * The whole months a PDF report covers: the months of the chosen range, or the span of the data when every date is wanted, or the
 * current month when there is no data.
 */
export function reportMonths(choice: Choice, span: Span, thisMonth: string): { fromMonth: string; toMonth: string } {
  if (choice.limitDates) {
    const { from, to } = bounds(choice)
    return { fromMonth: from.slice(0, 7) || thisMonth, toMonth: to.slice(0, 7) || thisMonth }
  }
  return { fromMonth: span.firstDate?.slice(0, 7) ?? thisMonth, toMonth: span.lastDate?.slice(0, 7) ?? thisMonth }
}

/**
 * What the file covers, as the app takes it. A PDF only ever covers whole months, so its dates are widened to them if need be, and it has
 * no other choice than the dates and the categories. A CSV is the transactions alone, so it has no parts to choose. Choices left as they
 * are come to nothing and are not sent.
 */
export function selectionFor(format: ExportFormat, choice: Choice): ExportSelection {
  const categoryIds = choice.allCategories ? null : choice.categoryIds
  let range: { from: string | null; to: string | null } = { from: null, to: null }
  if (choice.limitDates) {
    const { from, to } = bounds(choice)
    range = format === 'pdf' ? { from: from ? `${from.slice(0, 7)}-01` : from, to: to ? monthEnd(to.slice(0, 7)) : to } : { from, to }
  }
  const selection: ExportSelection = { ...range, categoryIds }
  if (format === 'pdf') return selection
  if (choice.kind) selection.kind = choice.kind
  if (choice.split) selection.split = choice.split
  const least = amountText(choice.minAmount)
  const most = amountText(choice.maxAmount)
  if (least) selection.minAmount = least
  if (most) selection.maxAmount = most
  const text = choice.text.trim()
  if (text) selection.text = text
  if (format === 'csv') return selection
  const sections = sectionsFor(format, choice)
  // a limit leaves the ledger out unless it is asked for, and choosing its part is asking
  if (isLimited(choice) && sections.includes('ledger')) selection.includeDebts = true
  const every = format === 'json' ? EXPORT_SECTIONS.filter((section) => section !== 'history' || !isLimited(choice)) : SHEETED
  if (sections.length !== every.length) selection.sections = sections
  return selection
}

/** The first thing wrong with the choice, or null when it can be exported. */
export function problemWith(format: ExportFormat, choice: Choice, months: number): Problem | null {
  if (choice.limitDates) {
    const { from, to } = bounds(choice)
    if (!from || !to || from > to) return 'dates'
  }
  if (!choice.allCategories && choice.categoryIds.length === 0) return 'categories'
  if (format === 'pdf' && months > MAX_REPORT_MONTHS) return 'months'
  if (format !== 'pdf') {
    const least = amountText(choice.minAmount)
    const most = amountText(choice.maxAmount)
    if (least === null || most === null) return 'amount'
    // the amounts are compared as cents, which two decimals make whole numbers
    if (least !== '' && most !== '' && Number(least.replace('.', '')) > Number(most.replace('.', ''))) return 'amount'
  }
  if ((format === 'json' || format === 'xlsx') && sectionsFor(format, choice).length === 0) return 'sections'
  return null
}
