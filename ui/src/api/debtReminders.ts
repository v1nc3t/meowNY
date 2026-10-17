import { invoke } from '@tauri-apps/api/core'
import { debts } from './debts'
import { t } from '../i18n/translate'
import { vault } from '../vault/api'
import { notifiedDays, reminderTime, ringNow, saveNotifiedDays, saveScheduledIds, scheduledIds, schedulePlan } from '../lib/reminders'

// Reminders about debts (docs/debts-design.md, section 11). The text is the same for everyone and holds a count, never a name or an
// amount: the app is locked when it rings, and a notification can be seen on a lock screen and kept by the system.
//
// On the computer the app rings while it is open and unlocked. On the phone the app hands the system the next few days, so they ring with
// the app closed. The system's notification plugin is called by its command names, like the camera, so the app has no extra package.

const PLUGIN = 'plugin:notification'

/** What the phone's scheduling came to. */
export type Scheduling = 'scheduled' | 'off' | 'denied' | 'unavailable'

/** Shows one notification for the friends whose reminder has come and which this device has not shown yet. Returns how many. */
export async function ringOnComputer(now = new Date()): Promise<number> {
  if (!(await vault.notifyDue())) return 0
  const reminders = await debts.reminders()
  const notified = notifiedDays()
  const ring = ringNow(reminders, reminderTime(), now, notified)
  // days of friends who are no longer due are forgotten, so the list does not grow
  const kept = Object.fromEntries(Object.entries(notified).filter(([id]) => reminders.due.some((friend) => friend.personId === id)))
  if (ring.length > 0) {
    await vault.showDueNotification(t('notify.debtTitle'), t('notify.debtBody', { count: ring.length }))
    for (const friend of ring) kept[friend.personId] = friend.dueOn
  }
  saveNotifiedDays(kept)
  return ring.length
}

async function allowed(): Promise<boolean> {
  const granted = await invoke<boolean | null>(`${PLUGIN}|is_permission_granted`)
  if (granted === true) return true
  // asked the first time there is something to ring about; a "no" is the person's and is only said, not asked again and again
  if (granted === false) return false
  return (await invoke<string>(`${PLUGIN}|request_permission`)) === 'granted'
}

async function cancelScheduled(): Promise<void> {
  const ids = scheduledIds()
  if (ids.length === 0) return
  await invoke(`${PLUGIN}|cancel`, { notifications: ids })
  saveScheduledIds([])
}

/**
 * Phone: cancels what was scheduled and schedules again the next days with a reminder, at the time of day of this device. Called at each
 * unlock, after each sync and when a reminder or a balance changes. A friend settled by a sync on a phone that is not opened keeps their
 * notification until the next unlock: that is said in the settings.
 */
export async function scheduleOnPhone(now = new Date()): Promise<Scheduling> {
  try {
    if (!(await vault.notifyDue())) {
      await cancelScheduled()
      return 'off'
    }
    const plan = schedulePlan(await debts.reminders(), reminderTime(), now)
    await cancelScheduled()
    if (plan.length === 0) return 'scheduled'
    if (!(await allowed())) return 'denied'
    for (const item of plan) {
      await invoke(`${PLUGIN}|notify`, {
        options: {
          id: item.id,
          title: t('notify.debtTitle'),
          body: t('notify.debtBody', { count: item.count }),
          schedule: { at: { date: item.at.toISOString(), repeating: false, allowWhileIdle: false } },
        },
      })
    }
    saveScheduledIds(plan.map((item) => item.id))
    return 'scheduled'
  } catch {
    return 'unavailable'
  }
}
