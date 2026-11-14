import { z } from 'zod'
import { call } from './invoke'

// Accounts, their balances and what moved them (docs/accounts-design.md, section 6.2). The app adds up; the screen only shows.
const id = z.string().regex(/^[1-9]\d*$/)
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
/** Money that may be below 0: a balance, an opening balance, what a line did to the account. */
const signed = z.string().regex(/^-?\d+\.\d{2}$/)
/** Money that never is: what a category added up to. */
const unsigned = z.string().regex(/^\d+\.\d{2}$/)

export const ACCOUNT_KINDS = ['CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD'] as const
export const AccountKind = z.enum(ACCOUNT_KINDS)
export type AccountKind = z.infer<typeof AccountKind>

export const Account = z.object({
  id,
  name: z.string(),
  kind: AccountKind,
  number: z.string().nullable(),
  openingBalance: signed,
  openingDate: date,
  archived: z.boolean(),
  /** The account that always stays: it can be neither archived nor deleted. */
  isMain: z.boolean(),
  /** The balance today. */
  balance: signed,
  changeThisMonth: signed,
  /** The latest day something moved it, the opening balance aside; null when nothing has. */
  lastActivity: date.nullable(),
  /** How many rows point at it. An account with none (that is not the main one) can be deleted. */
  entries: z.number().int().nonnegative(),
  /** The bank's BIC; null when there is none. */
  bic: z.string().nullable(),
  /** Free text, line breaks allowed; null when there is none. */
  notes: z.string().nullable(),
  /** The name of the group the account is listed under; null when it has none. */
  group: z.string().nullable(),
  /** Whether the net worth counts it. */
  includeInNetWorth: z.boolean(),
  /** Added to the balance to show what is available (a card's limit); it never enters the balance itself. */
  virtualBalance: signed,
  /** The balance plus the virtual balance. */
  available: signed,
})
export type Account = z.infer<typeof Account>

export type NewAccount = {
  name: string
  kind: AccountKind
  number?: string
  /** "0.00" when left out. */
  openingBalance?: string
  /** Today when left out. */
  openingDate?: string
  bic?: string
  notes?: string
  group?: string
  /** true when left out. */
  includeInNetWorth?: boolean
  /** "0.00" when left out. */
  virtualBalance?: string
}

/** Fields left out stay as they are; `null` clears `number`, `bic`, `notes` and `group`. */
export type AccountPatch = {
  name?: string
  kind?: AccountKind
  number?: string | null
  openingBalance?: string
  openingDate?: string
  bic?: string | null
  notes?: string | null
  group?: string | null
  includeInNetWorth?: boolean
  virtualBalance?: string
}

export const AccountsSummary = z.object({
  /** The balances of all accounts together, archived ones too. */
  inAccounts: signed,
  /** What friends owe you minus what you owe them. */
  friends: signed,
  netWorth: signed,
  accounts: z.number().int().nonnegative(),
  /** More than one account, or an opening balance on the main one: only then the dashboard says anything about accounts. */
  inUse: z.boolean(),
})
export type AccountsSummary = z.infer<typeof AccountsSummary>

export const LINE_SOURCES = ['OPENING', 'TRANSACTION', 'TRANSFER_IN', 'TRANSFER_OUT', 'LEDGER'] as const

/** One thing that moved an account. `id` is the transaction's, the transfer's, the ledger entry's or (opening) the account's. */
export const AccountLine = z.object({
  date,
  source: z.enum(LINE_SOURCES),
  id,
  /** The transaction's name, the transfer's note or the entry's note; empty when there is none. */
  what: z.string(),
  /** The category, the other account or the friend, by name. */
  other: z.string().nullable(),
  /** What it did to the account. */
  amount: signed,
  /** The balance after it. */
  balance: signed,
})
export type AccountLine = z.infer<typeof AccountLine>

export const AccountStatement = z.object({
  account: Account,
  from: date,
  to: date,
  balanceBefore: signed,
  /** Oldest first. */
  lines: z.array(AccountLine),
  balanceAfter: signed,
})
export type AccountStatement = z.infer<typeof AccountStatement>

/** The balance on the last day of a month ("YYYY-MM"); for the current month, today. */
export const HistoryPoint = z.object({ month: z.string().regex(/^\d{4}-(?:0[1-9]|1[0-2])$/), balance: signed })
export type HistoryPoint = z.infer<typeof HistoryPoint>

/** What one category added up to: its id, its name and the amount. */
export const Share = z.object({ categoryId: id, name: z.string(), amount: unsigned })
export type Share = z.infer<typeof Share>

/** What an account spent and took in over a period, by category (transfers and ledger entries are not in it): largest first. */
export const Breakdown = z.object({
  from: date,
  to: date,
  expenses: z.array(Share),
  income: z.array(Share),
  expenseTotal: unsigned,
  incomeTotal: unsigned,
})
export type Breakdown = z.infer<typeof Breakdown>

export const accounts = {
  /** The accounts in the person's order, then the archived ones. */
  list: () => call('list_accounts', z.array(Account)),
  create: (input: NewAccount) => call('create_account', Account, { input }),
  update: (id: string, patch: AccountPatch) => call('update_account', Account, { id, patch }),
  archive: (id: string) => call('archive_account', Account, { id }),
  unarchive: (id: string) => call('unarchive_account', Account, { id }),
  remove: (id: string) => call('delete_account', z.null(), { id }).then(() => undefined),
  /** The ids of all accounts that are not archived, in the order to keep. */
  reorder: (ids: string[]) => call('reorder_accounts', z.null(), { ids }).then(() => undefined),
  /** What moved the account from `from` to `to`, both days included. */
  statement: (id: string, from: string, to: string) => call('account_statement', AccountStatement, { id, from, to }),
  /** The months of a year ("YYYY") up to today, at most 12. */
  history: (id: string, year: string) => call('account_history', z.array(HistoryPoint), { id, year }),
  summary: () => call('accounts_summary', AccountsSummary),
  /** What the account spent and took in from `from` to `to`, both days included, by category. */
  breakdown: (id: string, from: string, to: string) => call('account_breakdown', Breakdown, { id, from, to }),
}
