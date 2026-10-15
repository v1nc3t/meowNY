import { useEffect } from 'react'
import { inDesktopApp, vault } from '../vault/api'
import { recurring } from '../api/recurring'
import { today } from '../api/today'
import { t } from '../i18n/translate'

const CHECK_EVERY_MS = 30 * 60 * 1000
const STORAGE_KEY = 'due-notified'
const NAMES_SHOWN = 3

/**
 * While the app is open and unlocked: shows one desktop notification a day listing the recurring
 * transactions that are due today or overdue, unless the person turned notifications off in Settings.
 * shortcut: nothing is shown for items that fall due later the same day, or when the app is closed.
 */
export function useDueNotifications(): void {
  useEffect(() => {
    if (!inDesktopApp()) return
    async function check() {
      try {
        if (!(await vault.notifyDue())) return
        const { date } = await today()
        if (localStorage.getItem(STORAGE_KEY) === date) return
        const { overdue, upcoming } = await recurring.dues(0)
        const due = [...overdue, ...upcoming]
        if (due.length === 0) return
        const shown = due.slice(0, NAMES_SHOWN).map((item) => item.name)
        const names = due.length > NAMES_SHOWN ? `${shown.join(', ')}, …` : shown.join(', ')
        await vault.showDueNotification(t('notify.title'), t('notify.body', { count: due.length, names }))
        localStorage.setItem(STORAGE_KEY, date)
      } catch {
        // No notification this round; the next check tries again.
      }
    }
    void check()
    const timer = setInterval(() => void check(), CHECK_EVERY_MS)
    return () => clearInterval(timer)
  }, [])
}
