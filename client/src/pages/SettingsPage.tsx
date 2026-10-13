import { useEffect, useState, type FormEvent } from 'react'
import { errorText, inDesktopApp, vault, type LockPrefs } from '../vault/api'
import PasswordField from '../vault/PasswordField'
import RecoveryKeyPanel from '../vault/RecoveryKeyPanel'
import { LOCK_AFTER_CHOICES, isCurrency, nearestLockChoice, normalizeCurrency, type Settings } from '../vault/rules'

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [prefs, setPrefs] = useState<LockPrefs | null>(null)
  const [location, setLocation] = useState('')
  const [message, setMessage] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)

  useEffect(() => {
    if (!inDesktopApp()) return
    vault.settings().then(setSettings, (e) => setMessage(errorText(e)))
    vault.lockPrefs().then(setPrefs, (e) => setMessage(errorText(e)))
    vault.dataLocation().then(setLocation, () => undefined)
  }, [])

  if (!inDesktopApp()) return <p>settings are available in the desktop app.</p>

  async function onSettings(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const next = {
      currency: normalizeCurrency(String(form.get('currency'))),
      locale: String(form.get('locale')).trim(),
      timezone: String(form.get('timezone')).trim(),
    }
    if (!isCurrency(next.currency)) return setMessage('use a three-letter currency code, like EUR.')
    try {
      await vault.saveSettings(next)
      setSettings(next)
      setMessage('settings saved. changing currency does not convert amounts.')
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
    <div>
      <h1>settings</h1>
      <p className="notice" role="status">
        {message}
      </p>

      {settings && (
        <form onSubmit={onSettings}>
          <h2>money and time</h2>
          <label>
            currency
            <input name="currency" required minLength={3} maxLength={3} defaultValue={settings.currency} />
          </label>
          <label>
            locale
            <input name="locale" required defaultValue={settings.locale} />
          </label>
          <label>
            timezone
            <input name="timezone" required defaultValue={settings.timezone} />
          </label>
          <button type="submit">save</button>
        </form>
      )}

      {prefs && (
        <form onSubmit={onLock}>
          <h2>app lock</h2>
          <label>
            lock after being idle for
            <select name="lock_after" defaultValue={nearestLockChoice(prefs.lock_after_secs)}>
              {LOCK_AFTER_CHOICES.map((choice) => (
                <option key={choice.seconds} value={choice.seconds}>
                  {choice.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            <input name="lock_on_minimize" type="checkbox" defaultChecked={prefs.lock_on_minimize} /> lock when the window is minimized
          </label>
          <label>
            <input name="lock_on_close" type="checkbox" defaultChecked={prefs.lock_on_close} /> lock when the window is closed
          </label>
          <button type="submit">save</button>
          <button type="button" onClick={() => void vault.lock().then(() => window.location.reload())}>
            lock now
          </button>
        </form>
      )}

      <section>
        <h2>recovery key</h2>
        {newKey ? (
          <RecoveryKeyPanel
            code={newKey}
            doneLabel="done"
            onDone={() => {
              setNewKey(null)
            }}
          />
        ) : (
          <form onSubmit={onRecoveryKey}>
            <p>
              the recovery key is the way back in if you forget your passphrase. making a new one stops the old one
              working.
            </p>
            <PasswordField label="passphrase (leave empty if you have none)" name="passphrase" current />
            <button type="submit">make a new recovery key</button>
          </form>
        )}
      </section>

      <section>
        <h2>your data</h2>
        {location && (
          <p>
            data file location: <code>{location}</code>
          </p>
        )}
        <form onSubmit={onWipe}>
          <h3>delete all my data</h3>
          <p>
            removes the data file, its key from the keychain and any backups made by this app. this cannot be undone.
            type DELETE to confirm.
          </p>
          <input value={confirmation} onChange={(e) => setConfirmation(e.target.value)} aria-label="type DELETE to confirm" autoComplete="off" />
          <button type="submit" disabled={confirmation !== 'DELETE'}>
            delete everything
          </button>
        </form>
      </section>
    </div>
  )
}
