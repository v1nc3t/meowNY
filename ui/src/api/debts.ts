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

// -- bill codes and reminders --------------------------------------------------------------------------------------------------

/** What a bill code holds, for the dialog before it is accepted. Reading it writes nothing. */
export const BillPreview = z.object({
  id: z.string().regex(/^[0-9a-f]{32}$/),
  from: z.string(),
  what: z.string(),
  date,
  /** The friend's part, in the currency of the code. */
  amount,
  total: amount,
  currency: z.string().regex(/^[A-Z]{3}$/),
  ownCurrency: z.string().regex(/^[A-Z]{3}$/),
  sameCurrency: z.boolean(),
  alreadyAdded: z.boolean(),
  /** The friend whose name is the sender's, to choose first. */
  friendId: id.nullable(),
})
export type BillPreview = z.infer<typeof BillPreview>

/** What the person chose in the dialog. Exactly one of `personId` and `newFriend`; `rate` and `amount` only for another currency. */
export type BillAccept = {
  text: string
  personId?: string
  newFriend?: string
  addExpense: boolean
  categoryId?: string
  rate?: string
  amount?: string
}

export const Reminders = z.object({
  today: date,
  /** The friends whose reminder day has come and whose balance is not 0, the oldest first. */
  due: z.array(z.object({ personId: id, name: z.string(), dueOn: date })),
  /** The next days with a reminder, today included, with how many friends have one. */
  upcoming: z.array(z.object({ date, count: z.number().int().positive() })),
})
export type Reminders = z.infer<typeof Reminders>

// -- the files -----------------------------------------------------------------------------------------------------------------

/** The fixed words of the text files and the PDF, in the person's language. */
export type DebtsLabels = {
  summaryTitle: string
  statementTitle: string
  /** The title of the history of debts in a report. */
  historyTitle: string
  /** The recurring payments that have come due, listed with the debts in a report, and their total. */
  dueTitle: string
  dueTotal: string
  /** What a recurring payment is called in the history of debts, when it was paid late and when it is overdue. */
  paidPayment: string
  duePayment: string
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

/** A recurring payment that has come due and is not paid or skipped: money you owe by now. One for each occurrence that was missed. */
export const DuePayment = z.object({
  recurringId: id,
  categoryId: id,
  name: z.string(),
  category: z.string(),
  /** The day it fell due. */
  dueDate: date,
  /** What it is for; null for a bill whose amount changes every time. */
  amount: amount.nullable(),
  /** Whole days since it fell due; 0 is today. */
  daysOverdue: z.number().int().nonnegative(),
})
export type DuePayment = z.infer<typeof DuePayment>

export const Due = z.object({
  /** Every occurrence, the oldest first. */
  payments: z.array(DuePayment),
  /** What the payments that say what they are for come to. */
  total: amount,
  /** How many say nothing about the amount. */
  unknown: z.number().int().nonnegative(),
})
export type Due = z.infer<typeof Due>

/** What a line of the history of debts is: the kinds of a friend's statement, and a recurring payment that was paid or is due. */
export const HISTORY_KINDS = ['LENT', 'BORROWED', 'REPAID_TO_ME', 'REPAID_BY_ME', 'BILL', 'PAYMENT_PAID', 'PAYMENT_DUE'] as const
export const HistoryKind = z.enum(HISTORY_KINDS)
export type HistoryKind = z.infer<typeof HistoryKind>

export const HistoryLine = z.object({
  /** The day it happened: for a recurring payment that was paid the day it was paid, for one that is due the day it fell due. */
  date,
  kind: HistoryKind,
  /** The friend, for a friend's line. */
  friend: z.string().nullable(),
  personId: id.nullable(),
  what: z.string(),
  /** Always above 0 (a payment that is due and has no fixed amount says 0.00). */
  amount,
  /** What it did to what the friend owes you, and what they owed after it. Only for a friend's line. */
  effect: signed.nullable(),
  balance: signed.nullable(),
  /** The day a recurring payment fell due. */
  dueDate: date.nullable(),
  entryId: id.nullable(),
  transactionId: id.nullable(),
  recurringId: id.nullable(),
})
export type HistoryLine = z.infer<typeof HistoryLine>

export const History = z.object({
  /** Newest first. */
  lines: z.array(HistoryLine),
  total: z.number().int().nonnegative(),
  limit: z.number().int().positive(),
  offset: z.number().int().nonnegative(),
})
export type History = z.infer<typeof History>

export type HistoryFilter = { from?: string; to?: string; personId?: string; limit?: number; offset?: number }

/** A file for the preview before it is saved: its bytes as base64url text, and how many. */
export const DebtsFilePreview = z.object({ data: z.string(), bytes: z.number().int().nonnegative() })
export type DebtsFilePreview = z.infer<typeof DebtsFilePreview>

export const debts = {
  /** The recurring payments that have come due and are not paid or skipped, and what they come to. */
  duePayments: () => call('debts_due_payments', Due),
  /** Everything that changed what you owe or are owed, newest first, a page at a time. */
  history: (filter: HistoryFilter = {}) => call('debts_history', History, { filter }),
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
  /** The text of the code for one friend's part of one bill. The same text every time; nothing is stored. */
  billCode: (transactionId: string, personId: string, from: string) => call('bill_code', z.string(), { transactionId, personId, from }),
  /** The code of a friend's part of the next payment of a recurring item that is split. */
  recurringCode: (recurringId: string, personId: string, from: string) => call('recurring_code', z.string(), { recurringId, personId, from }),
  billPreview: (text: string) => call('bill_preview', BillPreview, { text }),
  billAccept: (input: BillAccept) => call('bill_accept', Entry, { input }),
  /** The friends to be reminded of now and the days that come, for a period of `everyDays` days after each debt last changed (0: none). */
  reminders: (everyDays: number) => call('debts_reminders', Reminders, { everyDays }),
  /** Opens a save dialog and writes the file where the person says. False when they cancel. The file is not encrypted. */
  save: (request: DebtsFileRequest) => call('export_debts', z.boolean(), { request }),
  /** The file `save` would write, as base64url text, for the preview that comes before saving it. Written nowhere. */
  filePreview: (request: DebtsFileRequest) => call('export_debts_preview', DebtsFilePreview, { request }),
}
