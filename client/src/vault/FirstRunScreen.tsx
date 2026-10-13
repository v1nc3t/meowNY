import { useEffect, useState, type FormEvent } from 'react'
import { errorText, vault } from './api'
import { defaultSettings, isCurrency, normalizeCurrency, passphraseProblem } from './rules'

export default function FirstRunScreen({ onDone }: { onDone: () => void }) {
  const [location, setLocation] = useState('')
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)

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
      await vault.create({ ...defaults, currency: normalizeCurrency(currency) }, passphrase)
      onDone()
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>welcome to meowny</h1>
      <p>your data stays on this computer, in an encrypted file. nothing is sent anywhere.</p>

      <label>
        currency
        <input name="currency" required minLength={3} maxLength={3} placeholder="EUR" autoComplete="off" />
      </label>
      <p className="notice">changing it later does not convert existing amounts.</p>

      <label>
        passphrase (optional)
        <input name="passphrase" type="password" autoComplete="new-password" />
      </label>
      <label>
        repeat passphrase
        <input name="confirm" type="password" autoComplete="new-password" />
      </label>
      <p className="notice">
        without a passphrase the key lives in your operating system's keychain and the app opens when you sign in
        to this computer. with one, the file also needs the passphrase to open. a forgotten passphrase cannot be recovered.
      </p>

      {location && (
        <p className="notice">
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
  )
}
