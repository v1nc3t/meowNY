import { useEffect, useState, type FormEvent } from 'react'
import { errorText, vault } from './api'
import PasswordField from './PasswordField'
import RecoveryKeyPanel from './RecoveryKeyPanel'
import { defaultSettings, isCurrency, normalizeCurrency, passphraseProblem } from './rules'

export default function FirstRunScreen({ onDone }: { onDone: () => void }) {
  const [location, setLocation] = useState('')
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null)

  useEffect(() => {
    vault.dataLocation().then(setLocation, () => setLocation(''))
  }, [])

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const currency = String(form.get('currency'))
    const passphrase = String(form.get('passphrase'))
    const problem = passphraseProblem(passphrase, String(form.get('confirm')))
    if (!isCurrency(currency)) return setMessage('use a three-letter currency code, like EUR.')
    if (problem) return setMessage(problem)

    setBusy(true)
    try {
      const defaults = defaultSettings()
      // The data file exists from here on, but the app stays on this screen until the key is saved.
      setRecoveryKey(await vault.create({ ...defaults, currency: normalizeCurrency(currency) }, passphrase))
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  if (recoveryKey) {
    return (
      <main className="gate">
        <RecoveryKeyPanel code={recoveryKey} confirm doneLabel="continue" onDone={onDone} />
      </main>
    )
  }

  return (
    <main className="gate">
      <form onSubmit={onSubmit}>
        <p className="brand-mark">meowny</p>
        <h1>welcome</h1>
        <p className="hint">your data stays on this computer, encrypted. nothing is sent anywhere.</p>

        <label>
          currency
          <input name="currency" required minLength={3} maxLength={3} placeholder="EUR" autoComplete="off" />
        </label>
        <p className="hint">changing it later does not convert old amounts.</p>

        <PasswordField label="passphrase (optional)" name="passphrase" />
        <PasswordField label="repeat passphrase" name="confirm" />
        <p className="hint">
          leave it empty and the app opens when you log in to this computer. set one and you type it every time you
          unlock.
        </p>

        <p className="warning" role="note">
          you get a recovery key on the next screen. if you forget the passphrase, it is the only way back in. lose both
          and your data is gone for good.
        </p>

        {location && (
          <p className="hint">
            data file location: <code>{location}</code>
          </p>
        )}

        <button type="submit" disabled={busy}>
          create encrypted data file
        </button>
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
    </main>
  )
}
