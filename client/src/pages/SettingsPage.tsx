import { useEffect, useState, type FormEvent } from 'react'
import { errorText, inDesktopApp, vault, type LockPrefs } from '../vault/api'
import { currencyLabel } from '../api/currencies'
import ChangeCurrencyModal from '../components/ChangeCurrencyModal'
import Dropdown from '../components/Dropdown'
import PasswordField from '../vault/PasswordField'
import RecoveryKeyPanel from '../vault/RecoveryKeyPanel'
import { LOCK_AFTER_CHOICES, nearestLockChoice, type Settings } from '../vault/rules'
import { useNotice } from '../useNotice'

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [prefs, setPrefs] = useState<LockPrefs | null>(null)
  const [location, setLocation] = useState('')
  const [message, setMessage] = useNotice()
  const [confirmation, setConfirmation] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)
  const [lockAfter, setLockAfter] = useState('300')
  const [changingCurrency, setChangingCurrency] = useState(false)

  useEffect(() => {
    if (!inDesktopApp()) return
    vault.settings().then(setSettings, (e) => setMessage(errorText(e)))
    vault.lockPrefs().then(
      (loaded) => {
        setPrefs(loaded)
        setLockAfter(String(nearestLockChoice(loaded.lock_after_secs)))
      },
      (e) => setMessage(errorText(e)),
    )
    vault.dataLocation().then(setLocation, () => undefined)
  }, [setMessage])

  if (!inDesktopApp()) return <p>settings are available in the desktop app.</p>

  async function onSettings(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const next = {
      currency: settings?.currency ?? '',
      locale: String(form.get('locale')).trim(),
      timezone: String(form.get('timezone')).trim(),
    }
    try {
      await vault.saveSettings(next)
      setSettings(next)
      setMessage('saved.')
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
      setMessage('lock settings saved.')
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
        <h1>settings</h1>
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
            <h2>money and time</h2>
            <div className="settings-row">
              <span className="settings-row-label">currency</span>
              <span className="settings-row-value">{currencyLabel(settings.currency)}</span>
              <button type="button" onClick={() => setChangingCurrency(true)}>
                change
              </button>
            </div>
            <label>
              number and date format
              <input name="locale" required placeholder="en-IE" defaultValue={settings.locale} />
            </label>
            <label>
              timezone
              <input name="timezone" required placeholder="Europe/Dublin" defaultValue={settings.timezone} />
            </label>
            <div className="actions">
              <button type="submit">save</button>
            </div>
          </form>
        )}

        {prefs && (
          <form className="settings-block" onSubmit={onLock}>
            <h2>app lock</h2>
            <Dropdown
              label="lock after idle for"
              name="lock_after"
              value={lockAfter}
              options={LOCK_AFTER_CHOICES.map((choice) => ({ value: String(choice.seconds), label: choice.label }))}
              onChange={setLockAfter}
            />
            <label className="check">
              <input name="lock_on_minimize" type="checkbox" defaultChecked={prefs.lock_on_minimize} />
              lock when minimized
            </label>
            <label className="check">
              <input name="lock_on_close" type="checkbox" defaultChecked={prefs.lock_on_close} />
              lock when closed
            </label>
            <div className="actions">
              <button type="submit">save</button>
              <button type="button" onClick={() => void vault.lock().then(() => window.location.reload())}>
                lock now
              </button>
            </div>
          </form>
        )}

        <div className="settings-block">
          <h2>recovery key</h2>
          {newKey ? (
            <RecoveryKeyPanel code={newKey} inline doneLabel="done" onDone={() => setNewKey(null)} />
          ) : (
            <form onSubmit={onRecoveryKey}>
              <p className="hint">a new key replaces the old one. the old key stops working.</p>
              <PasswordField label="passphrase (if you set one)" name="passphrase" current />
              <div className="actions">
                <button type="submit">make new key</button>
              </div>
            </form>
          )}
        </div>

        <div className="settings-block">
          <h2>your data</h2>
          {location && (
            <p className="hint">
              stored in <code>{location}</code>
            </p>
          )}
          <form onSubmit={onWipe}>
            <p className="hint">deletes your data, key and backups. this cannot be undone.</p>
            <label>
              type DELETE to confirm
              <input value={confirmation} onChange={(e) => setConfirmation(e.target.value)} autoComplete="off" />
            </label>
            <div className="actions">
              <button type="submit" disabled={confirmation !== 'DELETE'}>
                delete all data
              </button>
            </div>
          </form>
        </div>
      </div>
    </section>
  )
}
