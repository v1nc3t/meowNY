import { useEffect } from 'react'
import { ringOnComputer, scheduleOnPhone } from '../api/debtReminders'
import { REMINDERS_CHANGED } from '../lib/reminders'
import { inDesktopApp } from '../vault/api'
import { useProfile } from './profile'

const CHECK_EVERY_MS = 30 * 60 * 1000

/**
 * Reminders about debts. On the computer: while the app is open and unlocked, one notification for the friends whose reminder has come
 * (at the time of day chosen here, or at once for one that came while the app was closed). On the phone: hands the system the next days,
 * at each unlock and whenever something that changes them happens.
 */
export function useDebtReminders(): void {
  const phone = useProfile() === 'phone'
  useEffect(() => {
    if (!inDesktopApp()) return
    const run = () => {
      const done = phone ? scheduleOnPhone() : ringOnComputer()
      // No notification this round: the next check tries again.
      void done.catch(() => undefined)
    }
    run()
    const timer = phone ? undefined : setInterval(run, CHECK_EVERY_MS)
    window.addEventListener(REMINDERS_CHANGED, run)
    return () => {
      if (timer) clearInterval(timer)
      window.removeEventListener(REMINDERS_CHANGED, run)
    }
  }, [phone])
}
