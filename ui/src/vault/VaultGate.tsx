import { useCallback, useEffect, useState, type ReactNode } from 'react'
import FirstRunScreen from './FirstRunScreen'
import LockScreen from './LockScreen'
import { PhoneFirstRun } from './PhoneScreens'
import { inDesktopApp, vault, type VaultStatus } from './api'
import { locksAfterBackground } from '../api/sync'
import { useProfile } from '../hooks/profile'
import { useI18n } from '../i18n/context'

const ACTIVITY_PING_MS = 15_000

/**
 * Shows the app only while the vault is unlocked. When the app is locked its pages are not
 * rendered at all, so no data stays in the page. Outside the desktop app (plain browser
 * development) there is no vault and the children show directly.
 */
export default function VaultGate({ children }: { children: ReactNode }) {
  const desktop = inDesktopApp()
  const phone = useProfile() === 'phone'
  const { setLanguage } = useI18n()
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

  // Once unlocked, show the app in the language saved with the data.
  useEffect(() => {
    if (!desktop || status !== 'unlocked') return
    vault.settings().then((saved) => setLanguage(saved.language), () => undefined)
  }, [desktop, status, setLanguage])

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

  // A phone is locked again when it comes back after the time the person chose: the idle timer cannot be trusted to run while
  // the app is in the background.
  useEffect(() => {
    if (!desktop || !phone || status !== 'unlocked') return
    let hiddenAt: number | null = null
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') {
        hiddenAt = Date.now()
        return
      }
      if (hiddenAt === null) return
      const away = Date.now() - hiddenAt
      hiddenAt = null
      vault.lockPrefs().then(
        (prefs) => {
          if (locksAfterBackground(away, prefs.lock_after_secs)) void vault.lock().then(() => setStatus('locked'), () => undefined)
        },
        () => undefined,
      )
    }
    document.addEventListener('visibilitychange', onVisibility)
    return () => document.removeEventListener('visibilitychange', onVisibility)
  }, [desktop, phone, status])

  if (status === 'loading') return null
  if (status === 'not_set_up') return phone ? <PhoneFirstRun onDone={refresh} /> : <FirstRunScreen onDone={refresh} />
  if (status === 'locked') return <LockScreen onUnlocked={refresh} />
  return <>{children}</>
}
