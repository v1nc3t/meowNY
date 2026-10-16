import { z } from 'zod'
import type { MessageKey } from '../i18n/en'
import { call } from './invoke'

// The phone and the sync (phase 8). The desktop shows a code and the phone scans it; this file is the UI's side of the
// commands that do it. `Status` is what `sync_status` answers: poll it while something runs, and show what it says.
// The Rust side never sends details of the data, only fixed words (`reason`) that the screens put in the person's language.

/** Which app this is. The desktop shows the codes; the phone scans them and keeps its data behind a passphrase. */
export const Profile = z.enum(['desktop', 'phone'])
export type Profile = z.infer<typeof Profile>

export const Purpose = z.enum(['pair', 'sync'])
export type Purpose = z.infer<typeof Purpose>

const count = z.number().int().nonnegative()

export const SummaryView = z.object({
  added: count,
  updated: count,
  replaced: count,
  deleted: count,
  merged: count,
  dropped: count,
})
export type SummaryView = z.infer<typeof SummaryView>

export const WarningView = z.object({ kind: z.literal('clock_off'), seconds: count })
export type WarningView = z.infer<typeof WarningView>

/** Which way a sync goes. The desktop chooses. */
export const MODES = ['both', 'to_desktop', 'to_phone'] as const
export const Mode = z.enum(MODES)
export type Mode = z.infer<typeof Mode>

export const TABLES = ['settings', 'categories', 'recurring_transactions', 'budgets', 'transactions'] as const
export const FIELDS = ['name', 'amount', 'date', 'category', 'description', 'type', 'month', 'every', 'nextDue', 'endDate', 'active', 'locale', 'timezone'] as const

/** One thing that was changed on both devices since they last met: how its two versions differ. */
export const Conflict = z.object({
  table: z.enum(TABLES),
  id: z.string(),
  title: z.string(),
  mineDeleted: z.boolean(),
  theirsDeleted: z.boolean(),
  fields: z.array(z.object({ field: z.enum(FIELDS), mine: z.string(), theirs: z.string() })),
})
export type Conflict = z.infer<typeof Conflict>

/** One of the states of a pairing or a sync, in the order they happen. */
export const Status = z.discriminatedUnion('state', [
  z.object({ state: z.literal('idle') }),
  /** The desktop shows `qr` and listens on `address`. */
  z.object({ state: z.literal('waiting'), purpose: Purpose, qr: z.string().min(1), address: z.string(), expiresInSecs: count }),
  /** The phone is dialling the desktop. */
  z.object({ state: z.literal('connecting'), purpose: Purpose }),
  /** Desktop, pairing: the six digits to type on the phone. */
  z.object({ state: z.literal('showCode'), purpose: Purpose, code: z.string().regex(/^\d{6}$/), peer: z.string() }),
  /** Phone, pairing: the person types the six digits that the desktop shows. */
  z.object({ state: z.literal('enterCode'), purpose: Purpose, triesLeft: count, wrong: z.boolean() }),
  z.object({ state: z.literal('running'), purpose: Purpose }),
  /** This device has conflicts to review; `more` further ones are settled by the newer change. */
  z.object({ state: z.literal('reviewing'), purpose: Purpose, conflicts: z.array(Conflict), more: count }),
  /** The other device is reviewing. */
  z.object({ state: z.literal('waitingReview'), purpose: Purpose }),
  z.object({
    state: z.literal('done'),
    purpose: Purpose,
    summary: SummaryView,
    warnings: z.array(WarningView),
    /** True when the two sides still differ after the sync (it should never be, unless it went one way). */
    differences: z.boolean(),
    /** The sync went one way, so the other device was not changed. */
    oneWay: z.boolean(),
    peer: z.string(),
  }),
  z.object({ state: z.literal('failed'), purpose: Purpose, reason: z.string().regex(/^[a-z_]+$/) }),
])
export type Status = z.infer<typeof Status>
export type State = Status['state']

const stamp = z.string().regex(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{3})?Z$/)

export const Peer = z.object({
  deviceId: z.string().regex(/^[0-9a-f]{32}$/),
  name: z.string(),
  pairedAt: stamp,
  lastSyncAt: stamp.nullable(),
  /** The currency was changed after the last sync with this device, so it shows the old one until the next sync. */
  currencyBehind: z.boolean(),
  /** How many changes were made on this device since the last sync with that one; null when they never synced. */
  changesWaiting: z.number().int().nonnegative().nullable(),
})
export type Peer = z.infer<typeof Peer>

/** How many wrong passphrases have been typed on a device that erases itself, and how many tries are left. */
export const Attempts = z.object({ failed: count, allowed: count, left: count })
export type Attempts = z.infer<typeof Attempts>

/** The longest a device name can be, as the Rust side allows it. */
export const DEVICE_NAME_MAX = 50

/** A device name as the app will keep it (trimmed), or null when it is empty, too long or has control characters. */
export function cleanDeviceName(name: string): string | null {
  const trimmed = name.trim()
  const length = [...trimmed].length
  // eslint-disable-next-line no-control-regex
  if (length === 0 || length > DEVICE_NAME_MAX || /[\u0000-\u001f\u007f-\u009f]/.test(trimmed)) return null
  return trimmed
}

export const syncApi = {
  profile: () => call('app_profile', Profile),
  attempts: () => call('unlock_attempts', Attempts.nullable()),
  status: () => call('sync_status', Status),
  /**
   * Desktop: opens a window for a new phone. The code is in the status. `name` is what the phone will call this computer.
   * `address` is another address of this computer on the home network, for one that is on several networks; empty is the system's choice.
   */
  startPairing: (name: string, address = '') => call('sync_start_pairing', z.null(), { name, address: address.trim() || null }).then(() => undefined),
  /** Phone: the person typed the six digits that the desktop shows. Three wrong tries end the pairing. */
  enterCode: (code: string) => call('sync_enter_code', z.null(), { code }).then(() => undefined),
  /** The person chose, for each conflict in the order shown, which version is true. */
  decide: (choices: Choice[]) => call('sync_decide', z.null(), { choices }).then(() => undefined),
  /** Desktop: opens a window for the paired phones. `mode` says which way the sync goes. */
  start: (name: string, address = '', mode: Mode = 'both') =>
    call('sync_start', z.null(), { name, address: address.trim() || null, mode }).then(() => undefined),
  /** Phone: the text of a scanned code. A pairing code pairs and a sync code syncs. */
  scan: (text: string, name: string) => call('sync_scan', z.null(), { text, name }).then(() => undefined),
  cancel: () => call('sync_cancel', z.null()).then(() => undefined),
  /** The screen has shown the end (done or failed); the status goes back to idle. */
  acknowledge: () => call('sync_acknowledge', z.null()).then(() => undefined),
  devices: () => call('sync_devices', z.array(Peer)),
  removeDevice: (deviceId: string) => call('sync_remove_device', z.boolean(), { deviceId }),
}

/** True while something is going on that the person can stop. */
export function isBusy(status: Status): boolean {
  return !(status.state === 'idle' || status.state === 'done' || status.state === 'failed')
}

/** True when the status is an end that the screen has to show once and then acknowledge. */
export function isFinished(status: Status): boolean {
  return status.state === 'done' || status.state === 'failed'
}

/** How often the screens ask for the status while something runs. */
export const POLL_MS = 500

/** The six digits as two groups of three, which are easier to compare by eye. */
export function checkCodeText(code: string): string {
  return code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code
}

/** Seconds as "m:ss", for the countdown. */
export function countdownText(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds))
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`
}

/** A gap between two clocks in words that need no translation: "45 s" or "3 min". */
export function clockGapText(seconds: number): string {
  return seconds < 120 ? `${seconds} s` : `${Math.round(seconds / 60)} min`
}

/** Every reason the Rust side can give, with the text for it. A reason this list does not know shows as `internal`. */
export const REASON_KEYS = {
  declined: 'sync.reason.declined',
  invalid: 'sync.reason.invalid',
  peer_version: 'sync.reason.peer_version',
  clock: 'sync.reason.clock',
  currency: 'sync.reason.currency',
  peer_busy: 'sync.reason.peer_busy',
  peer_error: 'sync.reason.peer_error',
  unreachable: 'sync.reason.unreachable',
  timeout: 'sync.reason.timeout',
  closed: 'sync.reason.closed',
  cancelled: 'sync.reason.cancelled',
  expired: 'sync.reason.expired',
  too_many_attempts: 'sync.reason.too_many_attempts',
  not_trusted: 'sync.reason.not_trusted',
  network: 'sync.reason.network',
  other_device_older: 'sync.reason.other_device_older',
  this_device_older: 'sync.reason.this_device_older',
  locked: 'sync.reason.locked',
  not_empty: 'sync.reason.not_empty',
  internal: 'sync.reason.internal',
  no_network: 'sync.reason.no_network',
  nobody_paired: 'sync.reason.nobody_paired',
} as const satisfies Record<string, MessageKey>

export function reasonKey(reason: string): MessageKey {
  return Object.hasOwn(REASON_KEYS, reason) ? REASON_KEYS[reason as keyof typeof REASON_KEYS] : REASON_KEYS.internal
}

/** The lines of a summary: which text and how many. Counts of zero are left out; `updated` already contains `replaced`. */
export function summaryLines(summary: SummaryView): { key: MessageKey; count: number }[] {
  const lines: { key: MessageKey; count: number }[] = [
    { key: 'sync.sumAdded', count: summary.added },
    { key: 'sync.sumUpdated', count: summary.updated },
    { key: 'sync.sumReplaced', count: summary.replaced },
    { key: 'sync.sumDeleted', count: summary.deleted },
    { key: 'sync.sumMerged', count: summary.merged },
    { key: 'sync.sumDropped', count: summary.dropped },
  ]
  return lines.filter((line) => line.count > 0)
}

/** True when the other device's changes changed nothing here. */
export function nothingChanged(summary: SummaryView): boolean {
  return summaryLines(summary).length === 0
}

/** From the 3rd wrong passphrase on, the lock screen says how many tries are left. */
export const WARN_FROM_FAILED = 3

export function shouldWarn(attempts: Attempts | null): boolean {
  return attempts !== null && attempts.failed >= WARN_FROM_FAILED && attempts.left > 0
}

/** The passphrase length from which the phone stops asking for a longer one. */
export const PHONE_PASSPHRASE_GOOD = 12

/** Whether to suggest a longer passphrase on the phone. An empty field gets no advice yet. */
export function wantsLongerPassphrase(passphrase: string): boolean {
  const length = [...passphrase].length
  return length > 0 && length < PHONE_PASSPHRASE_GOOD
}

/** The currency a new phone starts with. The first pairing replaces it with the desktop's, without converting anything. */
export const PHONE_PLACEHOLDER_CURRENCY = 'EUR'

/**
 * Whether a phone that has been in the background for `hiddenMs` locks when it comes back. `lockAfterSecs` is the person's
 * "lock after" choice; 0 means never lock by time.
 */
export function locksAfterBackground(hiddenMs: number, lockAfterSecs: number): boolean {
  return lockAfterSecs > 0 && hiddenMs >= lockAfterSecs * 1000
}


/** What the person decides for one conflict: the version on this device, or the one on the other. */
export type Choice = 'mine' | 'theirs'

/** The six digits as typed: spaces and anything that is not a digit are left out. */
export function cleanDigits(text: string): string {
  return text.replace(/\D/g, '').slice(0, 6)
}

/** Whether the choices cover every conflict (the person has to say for each one). */
export function allChosen(conflicts: readonly Conflict[], chosen: Readonly<Record<string, Choice | undefined>>): boolean {
  return conflicts.length > 0 && conflicts.every((conflict) => chosen[conflict.id] !== undefined)
}

/** The choices in the order of the conflicts, ready to send; null while one is missing. */
export function choicesInOrder(conflicts: readonly Conflict[], chosen: Readonly<Record<string, Choice | undefined>>): Choice[] | null {
  const out: Choice[] = []
  for (const conflict of conflicts) {
    const choice = chosen[conflict.id]
    if (choice === undefined) return null
    out.push(choice)
  }
  return out
}

/** "1 MONTH" as the number and the unit, for the words "every month" and "every 3 weeks". */
export function parseEvery(text: string): { count: number; unit: string } | null {
  const match = /^([1-9]\d{0,5}) (DAY|WEEK|MONTH|YEAR)$/.exec(text)
  return match ? { count: Number(match[1]), unit: match[2] } : null
}
