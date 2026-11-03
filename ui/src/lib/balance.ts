import type { Entry, Line } from '../api/debts'

/** Who owes whom, from a balance such as "-12.50" (negative: the person owes the friend). `abs` is the amount without its sign. */
export function balanceView(balance: string): { tone: 'owed' | 'owe' | 'even'; abs: string } {
  const abs = balance.replace('-', '')
  if (abs === '0.00') return { tone: 'even', abs }
  return { tone: balance.startsWith('-') ? 'owe' : 'owed', abs }
}

/** An entry as the edit dialog takes it, from its line in a friend's statement (the note of an entry is what the line says it is). */
export function entryOfLine(line: Line, personId: string): Entry | null {
  if (line.entryId === null || line.kind === 'BILL') return null
  return {
    id: line.entryId,
    personId,
    kind: line.kind,
    amount: line.amount,
    date: line.date,
    note: line.what === '' ? null : line.what,
    transactionId: line.transactionId,
  }
}

/** The kinds of line whose `what` is a name (of a bill or of a recurring payment). In an entry of the ledger it is the person's own note. */
const NAMED = new Set(['BILL', 'PAYMENT_PAID', 'PAYMENT_DUE'])

/** The note of a line, which a row shows in its panel; empty for a line with none, and for one whose `what` is a name (that is in its label). */
export function noteOf(line: { kind: string; what: string }): string {
  return NAMED.has(line.kind) ? '' : line.what
}
