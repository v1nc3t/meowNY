import { useState, type FormEvent } from 'react'
import { errorText, vault, type LockPrefs } from '../../vault/api'
import { currencyLabel } from '../../api/currencies'
import { formatExample, localeOptions, zoneOptions } from '../../api/locales'
import { useI18n } from '../../i18n/context'
import { languageOptions } from '../../i18n/languages'
import Dropdown from '../../components/Dropdown'
import Button from '../../components/ui/Button'
import { lockChoices, nearestLockChoice, type Settings } from '../../vault/rules'
import { scheduleOnPhone } from '../../api/debtReminders'
import { cleanCategoryName } from '../../api/categories'
import { saveSenderName, senderName } from '../../lib/senderName'
import { parseTheme, saveTheme, savedTheme } from '../../lib/theme'
import { parseTime, REMINDER_PERIODS, reminderEvery, reminderTime, remindersChanged, saveReminderEvery, saveReminderTime } from '../../lib/reminders'
import { Actions, Field, Row, Rows, SwitchRow } from './layout'

type Say = (text: string) => void

const PERIOD_LABELS = { 0: 'settings.debtEveryOff', 7: 'settings.scheduleWeekly', 14: 'settings.debtEveryTwoWeeks', 30: 'settings.scheduleMonthly' } as const

/** Language, number format, time zone and currency. */
export function RegionSection({
  settings,
  phone,
  onSaved,
  onChangeCurrency,
  say,
}: {
  settings: Settings
  phone: boolean
  onSaved: (next: Settings) => void
  onChangeCurrency: () => void
  say: Say
}) {
  const { t, setLanguage } = useI18n()
  const [chosenLanguage, setChosenLanguage] = useState(settings.language)
  const [locale, setLocale] = useState(settings.locale)
  const [timezone, setTimezone] = useState(settings.timezone)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const next = { currency: settings.currency, locale, timezone, language: chosenLanguage }
    try {
      await vault.saveSettings(next)
      onSaved(next)
      setLanguage(chosenLanguage)
      say(t('settings.saved'))
    } catch (error) {
      say(errorText(error))
    }
  }

  return (
    <form className="settings-form" onSubmit={onSubmit}>
      <Rows>
        <Field>
          <Dropdown label={t('settings.language')} value={chosenLanguage} options={languageOptions} onChange={setChosenLanguage} />
        </Field>
        <Row label={t('settings.format')} hint={formatExample(locale, settings.currency)}>
          <Dropdown compact label={t('settings.format')} value={locale} options={localeOptions(locale)} onChange={setLocale} />
        </Row>
        <Field>
          <Dropdown label={t('settings.timezone')} value={timezone} options={zoneOptions(timezone)} onChange={setTimezone} />
        </Field>
        <Row label={t('settings.currency')} hint={phone ? t('settings.currencyOnDesktop') : undefined}>
          <span className="settings-value">{currencyLabel(settings.currency)}</span>
          {phone ? null : (
            <Button variant="outline" size="sm" onClick={onChangeCurrency}>
              {t('common.change')}
            </Button>
          )}
        </Row>
      </Rows>
      <Actions>
        <Button type="submit">{t('common.save')}</Button>
      </Actions>
    </form>
  )
}

/** The theme is this device's and applies at once, on the phone as well. */
export function AppearanceSection() {
  const { t } = useI18n()
  const [theme, setTheme] = useState(savedTheme)
  return (
    <Rows>
      <Field>
        <Dropdown
          label={t('settings.theme')}
          value={theme}
          options={[
            { value: 'system', label: t('settings.themeSystem') },
            { value: 'light', label: t('settings.themeLight') },
            { value: 'dark', label: t('settings.themeDark') },
          ]}
          onChange={(value) => {
            const next = parseTheme(value)
            saveTheme(next)
            setTheme(next)
          }}
        />
      </Field>
    </Rows>
  )
}

export function AppLockSection({ prefs, phone, onSaved, say }: { prefs: LockPrefs; phone: boolean; onSaved: (next: LockPrefs) => void; say: Say }) {
  const { t } = useI18n()
  const [lockAfter, setLockAfter] = useState(() => String(nearestLockChoice(prefs.lock_after_secs, lockChoices(phone))))

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const next: LockPrefs = {
      lock_after_secs: Number(form.get('lock_after')),
      // A phone has no window to minimise or close: what is stored stays.
      lock_on_minimize: phone ? prefs.lock_on_minimize : form.get('lock_on_minimize') === 'on',
      lock_on_close: phone ? prefs.lock_on_close : form.get('lock_on_close') === 'on',
    }
    try {
      await vault.saveLockPrefs(next)
      onSaved(next)
      say(t('settings.saved'))
    } catch (error) {
      say(errorText(error))
    }
  }

  return (
    <form className="settings-form" onSubmit={onSubmit}>
      <Rows>
        <Field>
          <Dropdown
            label={t('settings.lockAfter')}
            name="lock_after"
            value={lockAfter}
            options={lockChoices(phone).map((choice) => ({ value: String(choice.seconds), label: t(choice.labelKey) }))}
            onChange={setLockAfter}
          />
        </Field>
        {phone ? null : (
          <>
            <SwitchRow label={t('settings.lockMinimized')} name="lock_on_minimize" defaultChecked={prefs.lock_on_minimize} />
            <SwitchRow label={t('settings.lockClosed')} name="lock_on_close" defaultChecked={prefs.lock_on_close} />
          </>
        )}
      </Rows>
      <Actions>
        <Button type="submit">{t('common.save')}</Button>
        <Button variant="outline" onClick={() => void vault.lock().then(() => window.location.reload())}>
          {t('settings.lockNow')}
        </Button>
      </Actions>
    </form>
  )
}

export function NotificationsSection({ notifyDue, phone, onNotify, say }: { notifyDue: boolean; phone: boolean; onNotify: (on: boolean) => void; say: Say }) {
  const { t } = useI18n()
  const [reminderAt, setReminderAt] = useState(reminderTime)
  const [debtEvery, setDebtEvery] = useState(() => String(reminderEvery()))

  return (
    <div className="settings-stack">
      <Rows>
        <SwitchRow label={t('settings.notifyDue')} checked={notifyDue} onChange={(event) => onNotify(event.target.checked)} />
        <Field>
          <label>
            {t('settings.reminderTime')}
            <input
              type="time"
              value={reminderAt}
              onChange={(event) => {
                setReminderAt(event.target.value)
                // the time of a reminder is this device's: kept here, and the phone hands the system the new time
                if (parseTime(event.target.value)) {
                  saveReminderTime(event.target.value)
                  remindersChanged()
                }
              }}
            />
          </label>
        </Field>
        <Field>
          <Dropdown
            label={t('settings.debtEvery')}
            value={debtEvery}
            options={REMINDER_PERIODS.map((days) => ({ value: String(days), label: t(PERIOD_LABELS[days]) }))}
            onChange={(value) => {
              setDebtEvery(value)
              // the period is this device's, like the time of day: kept here, and the phone hands the system the new days
              saveReminderEvery(Number(value))
              if (phone) {
                void scheduleOnPhone().then((result) => {
                  if (result === 'denied') say(t('debts.remindDenied'))
                })
              } else {
                remindersChanged()
              }
            }}
          />
        </Field>
      </Rows>
      <p className="hint">{t(phone ? 'settings.reminderPhone' : 'settings.reminderDesktop')}</p>
    </div>
  )
}

/** The name a friend sees, for the code of a shared bill. It stays on this device. */
export function FriendNameSection({ say }: { say: Say }) {
  const { t } = useI18n()
  const [name, setName] = useState(senderName)

  return (
    <form
      className="settings-form"
      onSubmit={(event) => {
        event.preventDefault()
        // the rule of a friend's name
        const clean = cleanCategoryName(name)
        if (!clean) return say(t('debts.errorName'))
        saveSenderName(clean)
        setName(clean)
        say(t('settings.saved'))
      }}
    >
      <Rows>
        <Field>
          <label>
            {t('code.senderAsk')}
            <input value={name} maxLength={50} autoComplete="name" onChange={(event) => setName(event.target.value)} />
          </label>
        </Field>
      </Rows>
      <Actions>
        <Button type="submit">{t('common.save')}</Button>
      </Actions>
    </form>
  )
}
