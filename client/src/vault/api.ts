import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Settings } from './rules'

export type VaultStatus = 'not_set_up' | 'locked' | 'unlocked'
export type LockPrefs = { lock_after_secs: number; lock_on_minimize: boolean; lock_on_close: boolean }

export type ErrorCode =
  | 'locked'
  | 'not_set_up'
  | 'already_set_up'
  | 'wrong_credentials'
  | 'key_unavailable'
  | 'database_too_new'
  | 'invalid_input'
  | 'internal'

const MESSAGES: Record<ErrorCode, string> = {
  locked: 'the app is locked.',
  not_set_up: 'the app is not set up yet.',
  already_set_up: 'the app is already set up.',
  wrong_credentials: 'wrong passphrase.',
  key_unavailable: 'the key is not in the OS keychain. the data file cannot be opened on this machine.',
  database_too_new: 'this data file was written by a newer version of meowny. update the app.',
  invalid_input: 'check the values and try again.',
  internal: 'something went wrong.',
}

export class VaultError extends Error {
  readonly code: ErrorCode
  constructor(code: ErrorCode) {
    super(MESSAGES[code])
    this.code = code
  }
}

/** True inside the desktop app. In a plain browser (client dev against the old server) there is no vault. */
export function inDesktopApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    const code = typeof error === 'string' && error in MESSAGES ? (error as ErrorCode) : 'internal'
    throw new VaultError(code)
  }
}

export function errorText(error: unknown): string {
  return error instanceof VaultError ? error.message : MESSAGES.internal
}

export const vault = {
  status: () => call<VaultStatus>('vault_status'),
  create: (settings: Settings, passphrase: string) =>
    call<void>('create_vault', { settings, passphrase: passphrase === '' ? null : passphrase }),
  unlock: (passphrase: string) => call<void>('unlock_vault', { passphrase: passphrase === '' ? null : passphrase }),
  lock: () => call<void>('lock_vault'),
  touch: () => call<void>('touch_activity'),
  settings: () => call<Settings>('get_settings'),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),
  lockPrefs: () => call<LockPrefs>('get_lock_prefs'),
  saveLockPrefs: (prefs: LockPrefs) => call<void>('save_lock_prefs', { prefs }),
  dataLocation: () => call<string>('data_location'),
  wipe: (confirmation: string) => call<void>('wipe_all_data', { confirmation }),
  onLocked: (handler: () => void) => listen('vault-locked', handler),
}
