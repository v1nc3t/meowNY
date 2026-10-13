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
const SettingsShape = z.object({ currency: z.string(), locale: z.string(), timezone: z.string() })
const LockPrefsShape = z.object({
  lock_after_secs: z.number().int(),
  lock_on_minimize: z.boolean(),
  lock_on_close: z.boolean(),
})

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
  unlock: (passphrase: string) => done(call('unlock_vault', Nothing, { passphrase: none(passphrase) })),
  lock: () => done(call('lock_vault', Nothing)),
  touch: () => done(call('touch_activity', Nothing)),
  settings: () => call('get_settings', SettingsShape),
  saveSettings: (settings: Settings) => done(call('save_settings', Nothing, { settings })),
  lockPrefs: () => call('get_lock_prefs', LockPrefsShape),
  saveLockPrefs: (prefs: LockPrefs) => done(call('save_lock_prefs', Nothing, { prefs })),
  dataLocation: () => call('data_location', z.string()),
  wipe: (confirmation: string) => done(call('wipe_all_data', Nothing, { confirmation })),
  onLocked: (handler: () => void) => listen('vault-locked', handler),
}
