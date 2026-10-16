import { listen } from '@tauri-apps/api/event'
import { z } from 'zod'
import { call, errorText, inDesktopApp, ApiError, type ErrorCode } from '../api/invoke'
import type { Settings } from './rules'

export { errorText, inDesktopApp, ApiError as VaultError }
export type { ErrorCode }

export type VaultStatus = 'not_set_up' | 'locked' | 'unlocked'
export type LockPrefs = { lock_after_secs: number; lock_on_minimize: boolean; lock_on_close: boolean }

const Nothing = z.null()
const Conversion = z.object({
  transactions: z.number().int().nonnegative(),
  recurring: z.number().int().nonnegative(),
  budgets: z.number().int().nonnegative(),
})
const RecoveryKey = z.string().regex(/^([0-9A-Z]{4}-){7}[0-9A-Z]{4}$/)
const Status = z.enum(['not_set_up', 'locked', 'unlocked'])
const SettingsShape = z.object({
  currency: z.string(),
  locale: z.string(),
  timezone: z.string(),
  language: z.string(),
})
const LockPrefsShape = z.object({
  lock_after_secs: z.number().int(),
  lock_on_minimize: z.boolean(),
  lock_on_close: z.boolean(),
})

const BackupSchedule = z.object({
  folder: z.string(),
  everyDays: z.number().int(),
  /** The files of the documents are in every backup, not only the rows. */
  includeDocuments: z.boolean(),
  /** UTC, ISO 8601. */
  lastAttemptAt: z.string().nullable(),
  lastOk: z.boolean(),
})
export type BackupSchedule = z.infer<typeof BackupSchedule>

/** What a backup holds of the documents: how many files are in it, and how many documents had no file on this device. */
export const BackupDone = z.object({ documents: z.number().int().nonnegative(), missing: z.number().int().nonnegative() })
export type BackupDone = z.infer<typeof BackupDone>

const none = (passphrase: string) => (passphrase === '' ? null : passphrase)
const done = <T>(promise: Promise<T>) => promise.then(() => undefined)

export const vault = {
  status: () => call('vault_status', Status),
  /** Creates the data file and returns the recovery key, which is shown once and never stored. */
  create: (settings: Settings, passphrase: string) =>
    call('create_vault', RecoveryKey, { settings, passphrase: none(passphrase) }),
  /** Unlocks with the recovery key and sets a new passphrase (empty means none). */
  recover: (recoveryKey: string, newPassphrase: string) =>
    done(call('recover_vault', Nothing, { recoveryKey, newPassphrase: none(newPassphrase) })),
  /**
   * Changes the currency. With a rate (units of the new currency per one of the current), every amount is
   * converted at that rate; with null only the label changes.
   */
  changeCurrency: (currency: string, rate: string | null) => call('change_currency', Conversion, { input: { currency, rate } }),
  /** Opens a save dialog and writes the key to a text file. False when the person cancels. */
  saveRecoveryKey: (code: string) => call('save_recovery_key', z.boolean(), { code }),
  /** Replaces the recovery key; the old one stops working. */
  newRecoveryKey: (passphrase: string) => call('new_recovery_key', RecoveryKey, { passphrase: none(passphrase) }),
  /**
   * Opens a save dialog and writes an encrypted backup locked with `passphrase`, with the files of the documents if asked. Null when
   * the person cancels.
   */
  createBackup: (passphrase: string, includeDocuments: boolean) =>
    call('create_backup', BackupDone.nullable(), { passphrase, includeDocuments }),
  /**
   * Opens a file dialog and replaces all data with the backup the person picks. `confirmation` must be "RESTORE".
   * False when the person cancels.
   */
  restoreBackup: (passphrase: string, confirmation: string) =>
    call('restore_backup', z.boolean(), { passphrase, confirmation }),
  /** First run: new data file filled from a backup. Returns the recovery key, or null when the person cancels. */
  restoreFromBackup: (backupPassphrase: string, passphrase: string) =>
    call('restore_backup_new_vault', RecoveryKey.nullable(), { passphrase: backupPassphrase, vaultPassphrase: none(passphrase) }),
  /** The scheduled backup setup, never its passphrase. Null when there is none. */
  backupSchedule: () => call('get_backup_schedule', BackupSchedule.nullable()),
  /**
   * Asks for a folder, makes a first backup there and then repeats every 1, 7 or 30 days while the app is open, with the files of the
   * documents if asked. False when the person cancels the folder dialog.
   */
  startBackupSchedule: (passphrase: string, everyDays: number, includeDocuments: boolean) =>
    call('set_backup_schedule', z.boolean(), { passphrase, everyDays, includeDocuments }),
  stopBackupSchedule: () => done(call('clear_backup_schedule', Nothing)),
  unlock: (passphrase: string) => done(call('unlock_vault', Nothing, { passphrase: none(passphrase) })),
  lock: () => done(call('lock_vault', Nothing)),
  touch: () => done(call('touch_activity', Nothing)),
  settings: () => call('get_settings', SettingsShape),
  saveSettings: (settings: Settings) => done(call('save_settings', Nothing, { settings })),
  lockPrefs: () => call('get_lock_prefs', LockPrefsShape),
  saveLockPrefs: (prefs: LockPrefs) => done(call('save_lock_prefs', Nothing, { prefs })),
  notifyDue: () => call('get_notify_due', z.boolean()),
  setNotifyDue: (on: boolean) => done(call('set_notify_due', Nothing, { on })),
  showDueNotification: (title: string, body: string) => done(call('show_due_notification', Nothing, { title, body })),
  dataLocation: () => call('data_location', z.string()),
  wipe: (confirmation: string) => done(call('wipe_all_data', Nothing, { confirmation })),
  onLocked: (handler: () => void) => listen('vault-locked', handler),
}
