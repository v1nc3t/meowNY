import { useCallback, useEffect, useState, type ReactNode } from 'react'
import FirstRunScreen from './FirstRunScreen'
import LockScreen from './LockScreen'
import { inDesktopApp, vault, type VaultStatus } from './api'

const ACTIVITY_PING_MS = 15_000

/**
 * Shows the app only while the vault is unlocked. When the app is locked its pages are not
 * rendered at all, so no data stays in the page. Outside the desktop app (plain browser
 * development) there is no vault and the children show directly.
 */
export default function VaultGate({ children }: { children: ReactNode }) {
  const desktop = inDesktopApp()
  const [status, setStatus] = useState<VaultStatus | 'loading'>(desktop ? 'loading' : 'unlocked')

  const refresh = useCallback(() => {
    vault.status().then(setStatus, () => setStatus('locked'))
  }, [])

  useEffect(() => {
    if (!desktop) return
    refresh()
    const stop = vault.onLocked(() => setStatus('locked'))
    return () => {
      void stop.then((unlisten) => unlisten())
    }
  }, [desktop, refresh])

  // Tell the idle timer the person is still here. Throttled; it carries no content.
  useEffect(() => {
    if (!desktop || status !== 'unlocked') return
    let last = 0
    const ping = () => {
      const now = Date.now()
      if (now - last < ACTIVITY_PING_MS) return
      last = now
      void vault.touch().catch(() => undefined)
    }
    const events = ['keydown', 'pointerdown', 'pointermove', 'wheel'] as const
    events.forEach((name) => window.addEventListener(name, ping, { passive: true }))
    return () => events.forEach((name) => window.removeEventListener(name, ping))
  }, [desktop, status])

  if (status === 'loading') return null
  if (status === 'not_set_up') return <FirstRunScreen onDone={refresh} />
  if (status === 'locked') return <LockScreen onUnlocked={refresh} />
  return <>{children}</>
}
