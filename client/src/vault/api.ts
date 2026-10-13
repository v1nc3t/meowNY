import { listen } from '@tauri-apps/api/event'
import { z } from 'zod'
import { call, errorText, inDesktopApp, ApiError, type ErrorCode } from '../api/invoke'
import type { Settings } from './rules'

export { errorText, inDesktopApp, ApiError as VaultError }
export type { ErrorCode }

export type VaultStatus = 'not_set_up' | 'locked' | 'unlocked'
export type LockPrefs = { lock_after_secs: number; lock_on_minimize: boolean; lock_on_close: boolean }

const Nothing = z.null()
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
  create: (settings: Settings, passphrase: string) =>
    done(call('create_vault', Nothing, { settings, passphrase: none(passphrase) })),
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
