import { useState } from 'react'
import { debts, type Person } from '../api/debts'
import { scheduleOnPhone } from '../api/debtReminders'
import { errorText } from '../api/invoke'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { remindersChanged } from '../lib/reminders'
import DatePicker from './DatePicker'

const MAX_DAYS = 365

/**
 * A friend's reminder: a day and, if wanted, a number of days after which it comes again. It counts while the friend owes something, and
 * it never says a name or an amount. Setting one on the phone hands it to the system at once (and asks for the permission the first time).
 */
export default function ReminderBlock({ person, today, onDone }: { person: Person; today: string; onDone: (message: string) => void }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const [on, setOn] = useState(person.remindOn ?? today)
  const [every, setEvery] = useState(person.remindEveryDays === null ? '' : String(person.remindEveryDays))
  const [pending, setPending] = useState(false)
  const [message, setMessage] = useState('')

  // after a change the phone schedules again (and says if the system will not let it ring); the computer rings by itself when it is open
  async function changed(): Promise<string> {
    if (!phone) {
      remindersChanged()
      return t('debts.remindSaved')
    }
    return (await scheduleOnPhone()) === 'denied' ? t('debts.remindDenied') : t('debts.remindSaved')
  }

  async function save() {
    const text = every.trim()
    const days = text === '' ? null : Number(text)
    if (days !== null && (!Number.isInteger(days) || days < 1 || days > MAX_DAYS)) return setMessage(t('debts.errorRepeat'))
    setPending(true)
    try {
      await debts.updatePerson(person.id, { remindOn: on, remindEveryDays: days })
      onDone(await changed())
    } catch (failure) {
      setMessage(errorText(failure))
    } finally {
      setPending(false)
    }
  }

  async function clear() {
    setPending(true)
    try {
      await debts.updatePerson(person.id, { remindOn: null })
      onDone(await changed())
    } catch (failure) {
      setMessage(errorText(failure))
    } finally {
      setPending(false)
    }
  }

  return (
    <div className="dash-card reminder-block">
      <h2>{t('debts.remind')}</h2>
      <p className="notice">{message}</p>
      <div className="tx-date-group">
        <span>{t('debts.remindOn')}</span>
        <DatePicker kind="date" value={on} label={t('debts.remindOn')} today={today} onChange={setOn} />
      </div>
      <label>
        {t('debts.remindEvery')}
        <input value={every} onChange={(event) => setEvery(event.target.value)} inputMode="numeric" maxLength={3} autoComplete="off" />
      </label>
      <p className="hint">{person.remindOn ? t('debts.remindHint') : t('debts.remindNone')}</p>
      <div className="card-actions">
        <button type="button" disabled={pending} onClick={() => void save()}>
          {t('common.save')}
        </button>
        {person.remindOn ? (
          <button type="button" disabled={pending} onClick={() => void clear()}>
            {t('debts.remindClear')}
          </button>
        ) : null}
      </div>
    </div>
  )
}
