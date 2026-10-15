import { checkPermissions, invoke, requestPermissions } from '@tauri-apps/api/core'
import { z } from 'zod'

// The camera of the phone, through the barcode scanner plugin of Tauri (added with the Android build). It is called by its
// command names, like every other command, so the app has no extra JavaScript package for it. It only exists on the phone.

const PLUGIN = 'barcode-scanner'

export type ScanResult =
  | { kind: 'text'; text: string }
  /** The camera is not allowed. */
  | { kind: 'denied' }
  /** The person stopped the scan. */
  | { kind: 'cancelled' }
  /** There is no scanner in this app (a desktop, or a build without the plugin). */
  | { kind: 'unavailable' }

const Scanned = z.object({ content: z.string() })
const Permission = z.object({ camera: z.string() })

let stopped = false

async function cameraAllowed(): Promise<boolean> {
  const now = Permission.safeParse(await checkPermissions<{ camera: string }>(PLUGIN))
  if (now.success && now.data.camera === 'granted') return true
  const asked = Permission.safeParse(await requestPermissions<{ camera: string }>(PLUGIN))
  return asked.success && asked.data.camera === 'granted'
}

/** Opens the camera and waits for one QR code. The page behind must be see-through while it runs (the `is-scanning` class). */
export async function scanQr(): Promise<ScanResult> {
  stopped = false
  try {
    if (!(await cameraAllowed())) return { kind: 'denied' }
  } catch {
    return { kind: 'unavailable' }
  }
  try {
    const scanned = Scanned.safeParse(await invoke(`plugin:${PLUGIN}|scan`, { formats: ['QR_CODE'], windowed: true }))
    if (stopped || !scanned.success || scanned.data.content === '') return { kind: 'cancelled' }
    return { kind: 'text', text: scanned.data.content }
  } catch {
    return { kind: 'cancelled' }
  }
}

export async function stopScan(): Promise<void> {
  stopped = true
  try {
    await invoke(`plugin:${PLUGIN}|cancel`)
  } catch {
    // Nothing was scanning.
  }
}
