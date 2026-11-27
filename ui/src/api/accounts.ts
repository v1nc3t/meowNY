import { z } from 'zod'
import { call } from './invoke'
import { Transaction } from './transactions'
import { Transfer } from './transfers'

// Accounts, their balances and what moved them (docs/accounts-design.md, section 6.2). The app adds up; the screen only shows.
const id = z.string().regex(/^[1-9]\d*$/)
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
/** Money that may be below 0: a balance, an opening balance, what a line did to the account. */
const signed = z.string().regex(/^-?\d+\.\d{2}$/)
/** Money that never is: what a category added up to. */
const unsigned = z.string().regex(/^\d+\.\d{2}$/)
/** A currency as three letters ("RON"). Capitals when the app stores it, but the check does not depend on that. */
const currency = z.string().regex(/^[A-Za-z]{3}$/)
/** A typed exchange rate as the app keeps it: "5.00", "0.0063". */
const rate = z.string().regex(/^\d+(?:\.\d+)?$/)

/** A percent as the app writes it, 0 to 100 with at most 3 decimals and no trailing zeros: "3.25", "4", "0.5". */
const percent = z.string().regex(/^(?:100|\d{1,2}(?:\.\d{1,3})?)$/)
/** A month ("YYYY-MM"). */
const month = z.string().regex(/^\d{4}-(?:0[1-9]|1[0-2])$/)

/** The kinds that hold money, and the kinds you owe on (docs/loans-deposits-design.md, section 2): a liability's balance is what is owed, below 0. */
export const ASSET_KINDS = ['CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD'] as const
export const LIABILITY_KINDS = ['CREDIT', 'LOAN', 'MORTGAGE'] as const
export const ACCOUNT_KINDS = [...ASSET_KINDS, ...LIABILITY_KINDS] as const
export const AccountKind = z.enum(ACCOUNT_KINDS)
export type AccountKind = z.infer<typeof AccountKind>

/** Whether an account of this kind is one you owe on (a credit, a loan, a mortgage). The others are assets. */
export function isLiability(kind: AccountKind): kind is (typeof LIABILITY_KINDS)[number] {
  return (LIABILITY_KINDS as readonly string[]).includes(kind)
}

/** What a rate is a percent of. */
export const INTEREST_PERIODS = ['MONTHLY', 'YEARLY'] as const
export const InterestPeriod = z.enum(INTEREST_PERIODS)
export type InterestPeriod = z.infer<typeof InterestPeriod>

/**
 * When a liability is paid off at its installment: `{ never: true }` when the installment does not beat a month's interest (or it would take over a
 * hundred years), else the payments still to make, the interest that adds up over them and the month of the last one.
 */
export const Payoff = z.union([z.object({ never: z.literal(true) }), z.object({ months: z.number().int().nonnegative(), interestLeft: unsigned, month })])
export type Payoff = z.infer<typeof Payoff>

export const LIABILITY_STATES = ['OWING', 'PAID_OFF', 'OVERPAID'] as const

/** What a liability owes today, worked out by the app from its balance, in the account's currency. */
export const Liability = z.object({
  /** The balance turned positive; 0 when it is 0 or above. */
  owed: unsigned,
  state: z.enum(LIABILITY_STATES),
  /** A month of interest on what is owed; null without a rate. */
  interestPerMonth: unsigned.nullable(),
  /** Null without an installment, and when nothing is owed. */
  payoff: Payoff.nullable(),
})
export type Liability = z.infer<typeof Liability>

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
  /** How many rows point at it, its stashes too. An account with none (that is not the main one) can be deleted. */
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
  /**
   * The currency its amounts are in: always a code, the app's own for an account in the app's currency. The balance, the change, the opening and
   * virtual balance, what is available, its statement, chart and breakdown are in it (docs/accounts-design.md, section 21).
   */
  currency,
  /** The account is in another currency than the app's: then it has a `rate`, and a transaction in it has two amounts. */
  foreign: z.boolean(),
  /** How many units of its currency one unit of the app's currency buys ("5.00" for 1 EUR = 5.00 RON); null for an account in the app's currency. */
  rate: rate.nullable(),
  /** The balance in the app's currency, at the rate: the same as `balance` for an account in the app's currency. */
  balanceInMain: signed,
  /** What the stashes in it hold, the archived ones too, in the account's currency; never below 0 (docs/stash-design.md, section 3). */
  setAside: unsigned,
  /** What no stash has set aside: the balance minus `setAside`. Below 0 when the stashes hold more than the account has. */
  free: signed,
  /** The percent the account earns or costs ("3.25"), and whether it is per month or per year; both null without a rate. Not the exchange `rate`. */
  interestRate: percent.nullable(),
  interestPeriod: InterestPeriod.nullable(),
  /** What a liability is paid each month ("950.00"); null for an asset and for a liability without one. */
  installment: unsigned.nullable(),
  /** What a liability owes; null for an asset. */
  liability: Liability.nullable(),
  /** What an asset with a rate returns in a month on what it holds; null without a rate and for a liability. */
  returnPerMonth: unsigned.nullable(),
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
  /** Left out, or the app's currency (any case): the account is in the app's currency and `rate` must be left out. Another code needs a `rate`. */
  currency?: string
  rate?: string
  /** A percent from "0" to "100" with at most 3 decimals; sent together with `interestPeriod`. */
  interestRate?: string
  interestPeriod?: InterestPeriod
  /** "0.01" and up, for a liability only. */
  installment?: string
}

/** Fields left out stay as they are; `null` clears `number`, `bic`, `notes` and `group`, and `interestRate` (with its period) and `installment`. */
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
  /** Only while nothing points at the account. The app's currency (any case) makes it an account in the app's currency and clears its rate. */
  currency?: string
  /** For an account in another currency, at any time; refused for one in the app's currency. */
  rate?: string
  /** What the account holds today, in its currency: the app sets the opening balance to make it so. Not together with `openingBalance`. */
  balance?: string
  /** A new rate needs a period, in this patch or already there; `null` clears the rate and its period. */
  interestRate?: string | null
  interestPeriod?: InterestPeriod
  /** For a liability only; `null` clears it. */
  installment?: string | null
}

export const AccountsSummary = z.object({
  /** The balances of the asset accounts together, archived ones too, in the app's currency (an account in another one at its rate). */
  inAccounts: signed,
  /** The balances of the liabilities that count in net worth, in the app's currency: below 0 while something is owed. */
  debts: signed,
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
  /** The currency of the amounts and balances of the lines: the account's. */
  currency,
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
  /** The currency of the amounts: the account's. */
  currency,
  from: date,
  to: date,
  expenses: z.array(Share),
  income: z.array(Share),
  expenseTotal: unsigned,
  incomeTotal: unsigned,
})
export type Breakdown = z.infer<typeof Breakdown>

/** One payment of a liability, split in two (docs/loans-deposits-design.md, section 5.4): the interest is an expense, the rest a transfer. */
export type Payment = {
  liabilityId: string
  fromAccountId: string
  /** The whole payment, interest included. */
  amount: string
  /** The part of `amount` that is interest, "0.00" when left out. */
  interest?: string
  /** An expense category; needed when the interest is above 0. */
  categoryId?: string
  /** Today when left out. */
  date?: string
  note?: string
}

/** What a payment made: the principal as a transfer (null when it was all interest) and the interest as an expense (null when there was none). */
export const Paid = z.object({ transfer: Transfer.nullable(), transaction: Transaction.nullable() })
export type Paid = z.infer<typeof Paid>

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
  /** Pays a liability: an expense for the interest and a transfer for the rest, in one step. */
  pay: (input: Payment) => call('accounts_pay', Paid, { input }),
  /** What the account spent and took in from `from` to `to`, both days included, by category. */
  breakdown: (id: string, from: string, to: string) => call('account_breakdown', Breakdown, { id, from, to }),
}
