import { useEffect, useState, type FormEvent } from 'react'
import { errorText, inDesktopApp, vault, type BackupSchedule, type LockPrefs } from '../vault/api'
import { currencyLabel } from '../api/currencies'
import { formatExample, localeOptions, whenText, zoneOptions } from '../api/locales'
import { useI18n } from '../i18n/context'
import { languageOptions } from '../i18n/languages'
import ChangeCurrencyModal from '../components/ChangeCurrencyModal'
import CommandApiBlock from '../components/CommandApiBlock'
import PhoneSyncBlock from '../components/PhoneSyncBlock'
import Dropdown from '../components/Dropdown'
import PasswordField from '../vault/PasswordField'
import RecoveryKeyPanel from '../vault/RecoveryKeyPanel'
import { backupPassphraseProblem, LOCK_AFTER_CHOICES, nearestLockChoice, type Settings } from '../vault/rules'
import { useNotice } from '../hooks/useNotice'
import { useProfile } from '../hooks/profile'
import { syncApi } from '../api/sync'

const EVERY_CHOICES = [
  { days: 1, labelKey: 'settings.scheduleDaily' },
  { days: 7, labelKey: 'settings.scheduleWeekly' },
  { days: 30, labelKey: 'settings.scheduleMonthly' },
] as const

export default function SettingsPage() {
  const { t, language, setLanguage } = useI18n()
  const phone = useProfile() === 'phone'
  const [chosenLanguage, setChosenLanguage] = useState(language)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [prefs, setPrefs] = useState<LockPrefs | null>(null)
  const [location, setLocation] = useState('')
  const [message, setMessage] = useNotice()
  const [confirmation, setConfirmation] = useState('')
  const [restoreWord, setRestoreWord] = useState('')
  const [busy, setBusy] = useState(false)
  const [schedule, setSchedule] = useState<BackupSchedule | null>(null)
  const [every, setEvery] = useState('7')
  const [newKey, setNewKey] = useState<string | null>(null)
  const [notifyDue, setNotifyDue] = useState<boolean | null>(null)
  const [lockAfter, setLockAfter] = useState('300')
  const [changingCurrency, setChangingCurrency] = useState(false)
  const [locale, setLocale] = useState('')
  const [timezone, setTimezone] = useState('')

  useEffect(() => {
    if (!inDesktopApp()) return
    vault.settings().then(
      (loaded) => {
        setSettings(loaded)
        setLocale(loaded.locale)
        setChosenLanguage(loaded.language)
        setTimezone(loaded.timezone)
      },
      (e) => setMessage(errorText(e)),
    )
    vault.lockPrefs().then(
      (loaded) => {
        setPrefs(loaded)
        setLockAfter(String(nearestLockChoice(loaded.lock_after_secs)))
      },
      (e) => setMessage(errorText(e)),
    )
    vault.notifyDue().then(setNotifyDue, (e) => setMessage(errorText(e)))
    vault.dataLocation().then(setLocation, () => undefined)
    vault.backupSchedule().then(setSchedule, (e) => setMessage(errorText(e)))
  }, [setMessage])

  if (!inDesktopApp()) return <p>{t('settings.unavailable')}</p>

  async function onSettings(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const next = {
      currency: settings?.currency ?? '',
      locale,
      timezone,
      language: chosenLanguage,
    }
    try {
      await vault.saveSettings(next)
      setSettings(next)
      setLanguage(chosenLanguage)
      setMessage(t('settings.saved'))
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  async function onLock(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const next: LockPrefs = {
      lock_after_secs: Number(form.get('lock_after')),
      lock_on_minimize: form.get('lock_on_minimize') === 'on',
      lock_on_close: form.get('lock_on_close') === 'on',
    }
    try {
      await vault.saveLockPrefs(next)
      setPrefs(next)
      setMessage(t('settings.saved'))
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  async function onNotify(on: boolean) {
    try {
      await vault.setNotifyDue(on)
      setNotifyDue(on)
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  async function onRecoveryKey(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const passphrase = String(new FormData(event.currentTarget).get('passphrase') ?? '')
    try {
      setNewKey(await vault.newRecoveryKey(passphrase))
      setMessage('')
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  async function onBackup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const passphrase = String(data.get('backup_passphrase'))
    const problem = backupPassphraseProblem(passphrase, String(data.get('backup_confirm')))
    if (problem) return setMessage(problem)
    setBusy(true)
    try {
      if (await vault.createBackup(passphrase)) {
        form.reset()
        setMessage(t('settings.backupSaved'))
      }
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  const everyLabel = (days: number) => {
    const choice = EVERY_CHOICES.find((c) => c.days === days)
    return choice ? t(choice.labelKey) : String(days)
  }

  async function onSchedule(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const passphrase = String(data.get('schedule_passphrase'))
    const problem = backupPassphraseProblem(passphrase, String(data.get('schedule_confirm')))
    if (problem) return setMessage(problem)
    setBusy(true)
    try {
      if (await vault.startBackupSchedule(passphrase, Number(every))) {
        form.reset()
        setSchedule(await vault.backupSchedule())
        setMessage(t('settings.scheduleStarted'))
      }
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  async function onStopSchedule() {
    try {
      await vault.stopBackupSchedule()
      setSchedule(null)
      setMessage(t('settings.scheduleStopped'))
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  async function onRestore(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const passphrase = String(new FormData(event.currentTarget).get('restore_passphrase'))
    setBusy(true)
    try {
      // Everything on screen is old data now, so start over from the top.
      if (await vault.restoreBackup(passphrase, restoreWord)) window.location.assign('/')
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  async function onWipe(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    try {
      await vault.wipe(confirmation)
      window.location.assign('/')
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  return (
    <section className="tx-page settings-page">
      <div className="tx-title">
        <h1>{t('settings.title')}</h1>
      </div>
      <p className="notice" role="status">
        {message}
      </p>

      {changingCurrency && settings ? (
        <ChangeCurrencyModal
          current={settings.currency}
          onClose={() => setChangingCurrency(false)}
          onChanged={(currency, summary) => {
            setSettings({ ...settings, currency })
            setChangingCurrency(false)
            // A paired phone still shows the old currency until the next sync, so say so.
            syncApi.devices().then(
              (paired) => setMessage(paired.length > 0 ? `${summary} ${t('currency.syncPhone')}` : summary),
              () => setMessage(summary),
            )
          }}
        />
      ) : null}

      <div className="tx-pane wide settings-grid">
        {settings && (
          <form className="settings-block" onSubmit={onSettings}>
            <h2>{t('settings.region')}</h2>
            <Dropdown
              label={t('settings.language')}
              value={chosenLanguage}
              options={languageOptions}
              onChange={setChosenLanguage}
            />
            <Dropdown
              label={t('settings.format')}
              value={locale}
              options={localeOptions(locale)}
              onChange={setLocale}
            />
            <p className="hint">{formatExample(locale, settings.currency)}</p>
            <Dropdown label={t('settings.timezone')} value={timezone} options={zoneOptions(timezone)} onChange={setTimezone} />
            <div className="settings-row">
              <span className="settings-row-label">{t('settings.currency')}</span>
              <span className="settings-row-value">{currencyLabel(settings.currency)}</span>
              {phone ? null : (
                <button type="button" onClick={() => setChangingCurrency(true)}>
                  {t('common.change')}
                </button>
              )}
            </div>
            {phone ? <p className="hint">{t('settings.currencyOnDesktop')}</p> : null}
            <div className="actions">
              <button type="submit">{t('common.save')}</button>
            </div>
          </form>
        )}

        {prefs && (
          <form className="settings-block" onSubmit={onLock}>
            <h2>{t('settings.appLock')}</h2>
            <Dropdown
              label={t('settings.lockAfter')}
              name="lock_after"
              value={lockAfter}
              options={LOCK_AFTER_CHOICES.map((choice) => ({ value: String(choice.seconds), label: t(choice.labelKey) }))}
              onChange={setLockAfter}
            />
            <label className="check">
              <input name="lock_on_minimize" type="checkbox" defaultChecked={prefs.lock_on_minimize} />
              {t('settings.lockMinimized')}
            </label>
            <label className="check">
              <input name="lock_on_close" type="checkbox" defaultChecked={prefs.lock_on_close} />
              {t('settings.lockClosed')}
            </label>
            <div className="actions">
              <button type="submit">{t('common.save')}</button>
              <button type="button" onClick={() => void vault.lock().then(() => window.location.reload())}>
                {t('settings.lockNow')}
              </button>
            </div>
          </form>
        )}

        {notifyDue !== null && (
          <div className="settings-block">
            <h2>{t('settings.notifications')}</h2>
            <label className="check">
              <input type="checkbox" checked={notifyDue} onChange={(event) => void onNotify(event.target.checked)} />
              {t('settings.notifyDue')}
            </label>
          </div>
        )}

        {settings && !phone && <PhoneSyncBlock locale={settings.locale} timezone={settings.timezone} />}

        {settings && !phone && <CommandApiBlock locale={settings.locale} timezone={settings.timezone} />}

        {/* A phone has no recovery key (it is not shown there): a forgotten passphrase means erasing the phone and pairing again. */}
        {!phone && (
          <div className="settings-block">
            <h2>{t('settings.recovery')}</h2>
            {newKey ? (
              <RecoveryKeyPanel code={newKey} inline doneLabel={t('common.done')} onDone={() => setNewKey(null)} />
            ) : (
              <form onSubmit={onRecoveryKey}>
                <p className="hint">{t('settings.recoveryHint')}</p>
                <PasswordField label={t('settings.recoveryPassphrase')} name="passphrase" current />
                <div className="actions">
                  <button type="submit">{t('settings.recoveryNew')}</button>
                </div>
              </form>
            )}
          </div>
        )}

        <div className="settings-block">
          <h2>{t('settings.backup')}</h2>
          <form onSubmit={onBackup}>
            <p className="hint">{t('settings.backupHint')}</p>
            <PasswordField label={t('settings.backupPassphrase')} name="backup_passphrase" />
            <PasswordField label={t('settings.backupPassphraseRepeat')} name="backup_confirm" />
            <div className="actions">
              <button type="submit" disabled={busy}>
                {t('settings.backupCreate')}
              </button>
            </div>
          </form>
          <form onSubmit={onSchedule}>
            <p className="hint">{t('settings.scheduleHint')}</p>
            {schedule && (
              <p className="hint">
                {t('settings.scheduleOn', { every: everyLabel(schedule.everyDays), folder: schedule.folder })}{' '}
                {schedule.lastAttemptAt &&
                  t(schedule.lastOk ? 'settings.scheduleLast' : 'settings.scheduleFailed', {
                    time: whenText(schedule.lastAttemptAt, locale, timezone),
                  })}
              </p>
            )}
            <PasswordField label={t('settings.backupPassphrase')} name="schedule_passphrase" />
            <PasswordField label={t('settings.backupPassphraseRepeat')} name="schedule_confirm" />
            <Dropdown
              label={t('settings.scheduleEvery')}
              value={every}
              options={EVERY_CHOICES.map((choice) => ({ value: String(choice.days), label: t(choice.labelKey) }))}
              onChange={setEvery}
            />
            <div className="actions">
              <button type="submit" disabled={busy}>
                {t('settings.scheduleStart')}
              </button>
              {schedule && (
                <button type="button" onClick={() => void onStopSchedule()}>
                  {t('settings.scheduleStop')}
                </button>
              )}
            </div>
          </form>
          <form onSubmit={onRestore}>
            <p className="hint">{t('settings.restoreHint')}</p>
            <PasswordField label={t('settings.backupPassphrase')} name="restore_passphrase" current />
            <label>
              {t('settings.restoreConfirm')}
              <input value={restoreWord} onChange={(e) => setRestoreWord(e.target.value)} autoComplete="off" />
            </label>
            <div className="actions">
              <button type="submit" disabled={busy || restoreWord !== 'RESTORE'}>
                {t('settings.restoreStart')}
              </button>
            </div>
          </form>
        </div>

        <div className="settings-block">
          <h2>{t('settings.data')}</h2>
          {location && (
            <p className="hint">
              {t('settings.storedIn')} <code>{location}</code>
            </p>
          )}
          <form onSubmit={onWipe}>
            <p className="hint">{t('settings.deleteHint')}</p>
            <label>
              {t('settings.deleteConfirm')}
              <input value={confirmation} onChange={(e) => setConfirmation(e.target.value)} autoComplete="off" />
            </label>
            <div className="actions">
              <button type="submit" disabled={confirmation !== 'DELETE'}>
                {t('settings.deleteAll')}
              </button>
            </div>
          </form>
        </div>
      </div>
    </section>
  )
}
