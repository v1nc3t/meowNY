import { z } from 'zod'
import { call } from './invoke'

// Stashes: money set aside inside one account for a goal (docs/stash-design.md, sections 6 and 13.7). The app adds up; the screen only shows.
// Every amount of a stash is in its account's currency. `saved` and `free` may be below 0 (a sync can leave them so, section 7.6); the rest never is.
const id = z.string().regex(/^[1-9]\d*$/)
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
/** Money that never is below 0: a target, what an entry moved, what is left. */
const money = z.string().regex(/^\d+\.\d{2}$/)
/** Money that may be: what a stash holds after a sync, what an account has free. */
const signed = z.string().regex(/^-?\d+\.\d{2}$/)
const currency = z.string().regex(/^[A-Za-z]{3}$/)
const count = z.number().int().nonnegative()

export const Stash = z.object({
  id,
  name: z.string(),
  accountId: id,
  /** The name of the account it lives in. */
  account: z.string(),
  /** The currency of every amount of the stash: its account's, the app's own for an account in the app's currency. */
  currency,
  archived: z.boolean(),
  accountArchived: z.boolean(),
  /** What it aims at; null when it has no target. */
  target: money.nullable(),
  targetDate: date.nullable(),
  /** Free text, line breaks allowed; null when there is none. */
  notes: z.string().nullable(),
  /** What its entries add up to. Below 0 or above the target only after a sync joined edits that were each fine alone (section 7.6). */
  saved: signed,
  /** What is missing to the target, never below 0; null without a target. */
  left: money.nullable(),
  /** `floor(saved * 100 / target)`: 0 when saved is not above 0, above 100 once the target is passed; null without a target. */
  percent: count.nullable(),
  reached: z.boolean(),
  /** What to save each month to reach the target by its date; null without a target or a date, when it is reached, and when the date is past. */
  perMonth: money.nullable(),
  overdue: z.boolean(),
  /** What its account has free, signed: below 0 when the stashes hold more than the account has. */
  free: signed,
  entries: count,
  lastEntry: date.nullable(),
})
export type Stash = z.infer<typeof Stash>

export const ENTRY_KINDS = ['ADD', 'TAKE'] as const
export const EntryKind = z.enum(ENTRY_KINDS)
export type EntryKind = z.infer<typeof EntryKind>

/** One add or take. `amount` is a size: the kind says which way it went. */
export const Entry = z.object({
  id,
  stashId: id,
  kind: EntryKind,
  amount: money,
  date,
  note: z.string().nullable(),
  /** The transfer that came with it; null when it came with none, or when that transfer was deleted. */
  transferId: id.nullable(),
  /** The other side of that transfer: the account the money came from for an add, the one it went to for a take. */
  otherAccountId: id.nullable(),
  otherAccount: z.string().nullable(),
  /** What the stash held after this entry (entries run oldest first by date, then id). */
  savedAfter: signed,
})
export type Entry = z.infer<typeof Entry>

/** A stash with its entries, newest first. The four calls that change an entry answer with this, so the page redraws from one answer. */
export const Detail = z.object({ stash: Stash, entries: z.array(Entry) })
export type Detail = z.infer<typeof Detail>

/** What a stash held at the end of a month ("YYYY-MM"); for the current month, today. */
export const HistoryPoint = z.object({ month: z.string().regex(/^\d{4}-(?:0[1-9]|1[0-2])$/), saved: signed })
export type HistoryPoint = z.infer<typeof HistoryPoint>

/** The stashes that are not archived, in the app's currency (a stash of a foreign account at its rate). */
export const Summary = z.object({
  stashes: count,
  /** What they hold: the sum of what each holds, never below 0 for one. */
  saved: money,
  /** The targets of those that have one. */
  target: money,
  /** How many have reached their target. */
  reached: count,
})
export type Summary = z.infer<typeof Summary>

/** `accountId` is required: a stash lives in one account. */
export type NewStash = { name: string; accountId: string; target?: string; targetDate?: string; notes?: string }

/** Fields left out stay as they are; `null` clears `target`, `targetDate` and `notes`. A new `accountId` moves the stash (same currency, enough free there). */
export type StashPatch = { name?: string; accountId?: string; target?: string | null; targetDate?: string | null; notes?: string | null }

/** Adds money; with `fromAccountId` a transfer from that account into the stash's account is made with it. `date` is today when left out. */
export type NewEntry = { amount: string; date?: string; note?: string; fromAccountId?: string }

/** Takes money out; with `toAccountId` a transfer from the stash's account to that one is made with it. */
export type NewTake = { amount: string; date?: string; note?: string; toAccountId?: string }

/** Fields left out stay as they are; `note: null` clears it. The amount is a size: an add stays an add. */
export type EntryPatch = { amount?: string; date?: string; note?: string | null }

export const stashes = {
  /** The stashes in the person's order, the archived ones last; with an account, only the stashes of that account. */
  list: (accountId?: string) => call('list_stashes', z.array(Stash), { accountId: accountId ?? null }),
  get: (id: string) => call('get_stash', Detail, { id }),
  create: (input: NewStash) => call('create_stash', Stash, { input }),
  update: (id: string, patch: StashPatch) => call('update_stash', Stash, { id, patch }),
  archive: (id: string) => call('archive_stash', Stash, { id }),
  unarchive: (id: string) => call('unarchive_stash', Stash, { id }),
  /** The stash and its entries go; the transfers that came with them stay. */
  remove: (id: string) => call('delete_stash', z.null(), { id }).then(() => undefined),
  /** The ids of all stashes that are not archived, in the order to keep. */
  reorder: (ids: string[]) => call('reorder_stashes', z.null(), { ids }).then(() => undefined),
  /** The months of a year ("YYYY") up to today, at most 12. */
  history: (id: string, year: string) => call('stash_history', z.array(HistoryPoint), { id, year }),
  summary: () => call('stashes_summary', Summary),
  add: (id: string, input: NewEntry) => call('add_to_stash', Detail, { id, input }),
  take: (id: string, input: NewTake) => call('take_from_stash', Detail, { id, input }),
  /** `id` is the entry's. */
  amend: (id: string, patch: EntryPatch) => call('update_stash_entry', Detail, { id, patch }),
  /** `id` is the entry's. With `deleteTransfer` the transfer that came with it is deleted too. */
  erase: (id: string, deleteTransfer = false) => call('delete_stash_entry', Detail, { id, deleteTransfer }),
}
