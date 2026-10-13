import { invoke } from '@tauri-apps/api/core'
import type { ZodType } from 'zod'

// The one place the UI talks to the Rust side. Every command goes through `call`:
// arguments go out as given, the answer is checked against a schema before the UI sees it,
// and failures come back as an ApiError with a fixed code (the Rust side never sends details).

export type ErrorCode =
  | 'locked'
  | 'not_set_up'
  | 'already_set_up'
  | 'wrong_credentials'
  | 'key_unavailable'
  | 'database_too_new'
  | 'invalid_input'
  | 'not_found'
  | 'name_exists'
  | 'internal'

export const MESSAGES: Record<ErrorCode, string> = {
  locked: 'the app is locked.',
  not_set_up: 'the app is not set up yet.',
  already_set_up: 'the app is already set up.',
  wrong_credentials: 'wrong passphrase.',
  key_unavailable: 'the key is not in the OS keychain. the data file cannot be opened on this machine.',
  database_too_new: 'this data file was written by a newer version of meowny. update the app.',
  invalid_input: 'check the values and try again.',
  not_found: 'that item no longer exists.',
  name_exists: 'that name already exists.',
  internal: 'something went wrong.',
}

export class ApiError extends Error {
  readonly code: ErrorCode
  constructor(code: ErrorCode) {
    super(MESSAGES[code])
    this.code = code
  }
}

export function toErrorCode(raw: unknown): ErrorCode {
  return typeof raw === 'string' && Object.hasOwn(MESSAGES, raw) ? (raw as ErrorCode) : 'internal'
}

/** True inside the desktop app. In a plain browser (client dev against the old server) there is no backend. */
export function inDesktopApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export function errorText(error: unknown): string {
  return error instanceof ApiError ? error.message : MESSAGES.internal
}

export async function call<T>(command: string, schema: ZodType<T>, args?: Record<string, unknown>): Promise<T> {
  let raw: unknown
  try {
    raw = await invoke(command, args)
  } catch (error) {
    throw new ApiError(toErrorCode(error))
  }
  const parsed = schema.safeParse(raw)
  if (!parsed.success) throw new ApiError('internal')
  return parsed.data
}
