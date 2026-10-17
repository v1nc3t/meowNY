import { z } from 'zod'
import type { DisplayFormat } from './displayFormat'
import { call } from './invoke'
import { Recurring } from './recurring'
import { PartOut, Transaction } from './transactions'

const id = z.string().regex(/^[1-9]\d*$/)
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
const amount = z.string().regex(/^\d+\.\d{2}$/)
/** An amount that may be below 0: a balance (positive when they owe you) or what a line did to it. */
const signed = z.string().regex(/^-?\d+\.\d{2}$/)

/** A friend: a name, a reminder, and what they owe you (negative when you owe them). Nothing else is kept about a person. */
export const Person = z.object({
  id,
  name: z.string(),
  archived: z.boolean(),
  remindOn: date.nullable(),
  remindEveryDays: z.number().int().positive().nullable(),
  balance: signed,
})
export type Person = z.infer<typeof Person>

export const Summary = z.object({
  /** The sum of the balances above 0: what friends owe you. */
  owedToYou: amount,
  /** The sum of the balances below 0, as a positive number: what you owe friends. */
  youOwe: amount,
  net: signed,
  /** The friends whose balance is not 0, the largest first. */
  people: z.array(Person),
})
export type Summary = z.infer<typeof Summary>

export const ENTRY_KINDS = ['LENT', 'BORROWED', 'REPAID_TO_ME', 'REPAID_BY_ME'] as const
export const EntryKind = z.enum(ENTRY_KINDS)
export type EntryKind = z.infer<typeof EntryKind>

export const Entry = z.object({
  id,
  personId: id,
  kind: EntryKind,
  amount,
  date,
  note: z.string().nullable(),
  transactionId: id.nullable(),
})
export type Entry = z.infer<typeof Entry>

export type NewEntry = {
  personId: string
  kind: EntryKind
  amount: string
  date: string
  note?: string | null
  transactionId?: string | null
}

/** Fields left out stay as they are; `note: null` and `transactionId: null` clear them. */
export type EntryPatch = Partial<NewEntry>

/** The expense that "I borrowed" can add to the person's expenses along with the entry. */
export type ExpenseInput = { name: string; categoryId: string }

export const Settled = z.object({ entry: Entry, before: signed, after: signed, over: amount })
export type Settled = z.infer<typeof Settled>

/** One line of a friend's ledger: their part of a bill, or an entry. `BILL` or one of the entry kinds. */
export const Line = z.object({
  date,
  kind: z.enum(['BILL', ...ENTRY_KINDS]),
  what: z.string(),
  amount,
  effect: signed,
  balance: signed,
  entryId: id.nullable(),
  transactionId: id.nullable(),
})
export type Line = z.infer<typeof Line>

export const Statement = z.object({ person: Person, lines: z.array(Line) })
export type Statement = z.infer<typeof Statement>

// -- sharing a bill ----------------------------------------------------------------------------------------------------------

export const SPLIT_MODES = ['EQUAL', 'PERCENT', 'SHARES', 'EXACT'] as const
export const SplitMode = z.enum(SPLIT_MODES)
export type SplitMode = z.infer<typeof SplitMode>

/** One friend or guest on a bill, as typed. Exactly one of `personId` and `guest`. */
export type PartInput = { personId?: string; guest?: string; value?: string }
/** A split as it is typed: the mode, the person's own value and the others. With `mine` left out, percent and exact give the rest. */
export type SplitInput = { mode: SplitMode; mine?: string; parts: PartInput[] }

export { PartOut }

export const SplitPreview = z.object({
  complete: z.boolean(),
  /** What is left to assign: percent in percent mode, money in exact mode. Negative when over. */
  left: signed,
  mine: amount,
  parts: z.array(PartOut),
})
export type SplitPreview = z.infer<typeof SplitPreview>

// -- the files -----------------------------------------------------------------------------------------------------------------

/** The fixed words of the text files and the PDF, in the person's language. */
export type DebtsLabels = {
  summaryTitle: string
  statementTitle: string
  owedToYou: string
  youOwe: string
  net: string
  owesYou: string
  youOweThem: string
  nobody: string
  settled: string
  lent: string
  borrowed: string
  repaidToMe: string
  repaidByMe: string
  bill: string
  date: string
  what: string
  change: string
  balance: string
  name: string
  amount: string
  page: string
}

export type DebtsFileRequest = {
  what: 'SUMMARY' | 'STATEMENT' | 'LEDGER'
  format: 'TEXT' | 'CSV' | 'PDF'
  personId?: string
  labels: DebtsLabels
  display?: DisplayFormat
  page?: 'A4' | 'LETTER'
}

export const debts = {
  summary: () => call('debts_summary', Summary),
  people: (archived = false) => call('list_people', z.array(Person), { archived }),
  createPerson: (name: string) => call('create_person', Person, { name }),
  /** `remindOn: null` clears the reminder and its repeat; `remindEveryDays: null` makes it once. */
  updatePerson: (id: string, patch: { name?: string; remindOn?: string | null; remindEveryDays?: number | null }) =>
    call('update_person', Person, { id, patch }),
  archive: (id: string) => call('archive_person', Person, { id }),
  unarchive: (id: string) => call('unarchive_person', Person, { id }),
  statement: (id: string) => call('person_statement', Statement, { id }),
  addEntry: (input: NewEntry, expense?: ExpenseInput) => call('add_ledger_entry', Entry, { input, expense: expense ?? null }),
  updateEntry: (id: string, patch: EntryPatch) => call('update_ledger_entry', Entry, { id, patch }),
  removeEntry: (id: string) => call('delete_ledger_entry', z.null(), { id }).then(() => undefined),
  /** "Mark as paid". No amount means the whole balance; one above the debt is accepted and the answer says how much was over. */
  settle: (id: string, options: { amount?: string; date?: string; note?: string } = {}) =>
    call('settle_person', Settled, { id, amount: options.amount ?? null, date: options.date ?? null, note: options.note ?? null }),
  preview: (amount: string, input: SplitInput) => call('split_preview', SplitPreview, { amount, input }),
  split: (transactionId: string, input: SplitInput) => call('set_transaction_split', Transaction, { id: transactionId, input }),
  unsplit: (transactionId: string) => call('clear_transaction_split', Transaction, { id: transactionId }),
  splitRecurring: (recurringId: string, input: SplitInput) => call('set_recurring_split', Recurring, { id: recurringId, input }),
  unsplitRecurring: (recurringId: string) => call('clear_recurring_split', Recurring, { id: recurringId }),
  /** Opens a save dialog and writes the file where the person says. False when they cancel. The file is not encrypted. */
  save: (request: DebtsFileRequest) => call('export_debts', z.boolean(), { request }),
}
