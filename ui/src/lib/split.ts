import type { PartInput, SplitInput, SplitMode } from '../api/debts'
import type { PartOut } from '../api/transactions'
import type { Template } from '../api/recurring'
import { amountString } from './money'

/**
 * What the bill dialog holds while the person types. The rule that turns these numbers into amounts is in Rust (`split_preview`); this
 * file only gets the typed text into the shape Rust takes, and back from a saved bill.
 */
export type SplitRow = {
  /** Only for the list on the screen. */
  key: string
  personId?: string
  guest?: string
  /** The name shown: the friend's, or the guest's. */
  name: string
  /** What was typed for this person, in the way the mode says. Empty for an equal split. */
  value: string
}

export type SplitDraft = {
  mode: SplitMode
  /** What was typed for the person themself. */
  mine: string
  /** Percent and exact give the person what the others leave. */
  mineIsRest: boolean
  rows: SplitRow[]
}

/** The bill dialog of a transaction or recurring item being made or changed: whether it is shared, how, whether the numbers add up, and whether the person touched them. */
export type SplitState = { on: boolean; draft: SplitDraft; ready: boolean; touched: boolean }

let serial = 0
export const rowKey = (): string => `row-${(serial += 1)}`

/** A bill that starts as an equal split between the person and these. */
export function equalDraft(rows: Omit<SplitRow, 'key' | 'value'>[] = []): SplitDraft {
  return { mode: 'EQUAL', mine: '', mineIsRest: true, rows: rows.map((row) => ({ ...row, key: rowKey(), value: '' })) }
}

/** A saved bill opens in the exact mode, with the amounts as they are stored; the person's own part is the rest. */
export function draftOfBill(split: PartOut[]): SplitDraft {
  return {
    mode: 'EXACT',
    mine: '',
    mineIsRest: true,
    rows: split.map((part) => ({
      key: rowKey(),
      ...(part.personId ? { personId: part.personId } : {}),
      ...(part.guest ? { guest: part.guest } : {}),
      name: part.person ?? part.guest ?? '',
      value: part.amount,
    })),
  }
}

/** Hundredths as a number with two decimals: 1900 is "19.00". */
function hundredths(weight: number): string {
  return `${Math.floor(weight / 100)}.${String(weight % 100).padStart(2, '0')}`
}

/**
 * A saved template is kept as weights, so what was typed has to be guessed back. Weights that make up the whole of a percent split
 * (10,000 hundredths) open as percent, so 19 and 81 come back as 19.00 and 81.00. Any others open as shares in lowest terms, so a split
 * of 19.00 and 1.00 (kept as 1900 and 100) comes back as 19 and 1. The proportions are the same whatever it was typed as.
 */
export function draftOfTemplate(template: Template): SplitDraft {
  const weights = [template.mine, ...template.parts.map((part) => part.weight)]
  const percent = weights.reduce((sum, weight) => sum + weight, 0) === 10_000
  const shown = percent ? weights.map(hundredths) : reducedWeights(weights).map(String)
  return {
    mode: percent ? 'PERCENT' : 'SHARES',
    mine: shown[0] ?? '',
    mineIsRest: false,
    rows: template.parts.map((part, at) => ({
      key: rowKey(),
      ...(part.personId ? { personId: part.personId } : {}),
      ...(part.guest ? { guest: part.guest } : {}),
      name: part.person ?? part.guest ?? '',
      value: shown[at + 1] ?? '',
    })),
  }
}

/** The mode changes: shares start at 1 for everyone, percent and exact start empty, and the person has the rest. */
export function withMode(draft: SplitDraft, mode: SplitMode): SplitDraft {
  if (mode === draft.mode) return draft
  const value = mode === 'SHARES' ? '1' : ''
  return { ...draft, mode, mine: value, mineIsRest: mode !== 'SHARES', rows: draft.rows.map((row) => ({ ...row, value })) }
}

/** The text of one number as Rust takes it: whole numbers for shares, two decimals for percent and exact. Null when it is not a number. */
export function normalise(mode: SplitMode, raw: string): string | null {
  const text = raw.trim()
  if (mode === 'SHARES') return /^\d{1,12}$/.test(text) ? text : null
  // an empty field counts as nothing while the person is still typing
  return text === '' ? '0.00' : amountString(text, true)
}

/** The draft as Rust takes it, or null when a number is not yet a number (the dialog then shows nothing new). */
export function toInput(draft: SplitDraft): SplitInput | null {
  if (draft.rows.length === 0) return null
  const parts: PartInput[] = []
  for (const row of draft.rows) {
    const part: PartInput = row.personId ? { personId: row.personId } : { guest: row.guest ?? row.name }
    if (draft.mode !== 'EQUAL') {
      const value = normalise(draft.mode, row.value)
      if (value === null) return null
      part.value = value
    }
    parts.push(part)
  }
  if (draft.mode === 'EQUAL') return { mode: 'EQUAL', parts }
  if (draft.mineIsRest && draft.mode !== 'SHARES') return { mode: draft.mode, parts }
  const mine = normalise(draft.mode, draft.mine)
  return mine === null ? null : { mode: draft.mode, mine, parts }
}

/** The people on a shared bill, to show next to its amount: "Anna 25.00, Bob 25.00". */
export function partsText(split: PartOut[], format: (amount: string) => string = (amount) => amount): string {
  return split.map((part) => `${part.person ?? part.guest ?? ''} ${format(part.amount)}`).join(', ')
}

/** The weights of a template in lowest terms, so 5000 and 5000 read as 1 and 1: only for showing, the weights stay as they are kept. */
export function reducedWeights(weights: number[]): number[] {
  const gcd = (a: number, b: number): number => (b === 0 ? a : gcd(b, a % b))
  const divisor = weights.reduce((common, weight) => gcd(common, weight), 0)
  return divisor > 1 ? weights.map((weight) => weight / divisor) : weights
}

export const NOT_SPLIT: SplitState = { on: false, draft: equalDraft(), ready: false, touched: false }
