import { useEffect, useState, type FormEvent } from 'react'
import { errorText, inDesktopApp, vault, type LockPrefs } from '../vault/api'
import { currencyLabel } from '../api/currencies'
import { formatExample, localeOptions, zoneOptions } from '../api/locales'
import { useI18n } from '../i18n/context'
import { languageOptions } from '../i18n/languages'
import ChangeCurrencyModal from '../components/ChangeCurrencyModal'
import Dropdown from '../components/Dropdown'
import PasswordField from '../vault/PasswordField'
import RecoveryKeyPanel from '../vault/RecoveryKeyPanel'
import { LOCK_AFTER_CHOICES, nearestLockChoice, type Settings } from '../vault/rules'
import { useNotice } from '../useNotice'

export default function SettingsPage() {
  const { t, language, setLanguage } = useI18n()
  const [chosenLanguage, setChosenLanguage] = useState(language)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [prefs, setPrefs] = useState<LockPrefs | null>(null)
  const [location, setLocation] = useState('')
  const [message, setMessage] = useNotice()
  const [confirmation, setConfirmation] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)
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
    vault.dataLocation().then(setLocation, () => undefined)
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
            setMessage(summary)
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
              <button type="button" onClick={() => setChangingCurrency(true)}>
                {t('common.change')}
              </button>
            </div>
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
