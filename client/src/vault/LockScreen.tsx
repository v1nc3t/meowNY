import { useEffect, useState, type FormEvent } from 'react'
import { errorText, vault } from './api'

export default function LockScreen({ onUnlocked }: { onUnlocked: () => void }) {
  const [needsPassphrase, setNeedsPassphrase] = useState(false)
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)

  // Try without a passphrase first: a keychain-only vault opens straight away.
  useEffect(() => {
    let cancelled = false
    vault.unlock('').then(
      () => {
        if (!cancelled) onUnlocked()
      },
      (error) => {
        if (cancelled) return
        if (error?.code === 'wrong_credentials') setNeedsPassphrase(true)
        else setMessage(errorText(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [onUnlocked])

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const passphrase = String(new FormData(event.currentTarget).get('passphrase'))
    setBusy(true)
    try {
      await vault.unlock(passphrase)
      onUnlocked()
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>meowny is locked</h1>
      {needsPassphrase && (
        <>
          <label>
            passphrase
            <input name="passphrase" type="password" autoFocus autoComplete="current-password" />
          </label>
          <button type="submit" disabled={busy}>
            unlock
          </button>
        </>
      )}
      <p className="notice" role="alert">
        {message}
      </p>
    </form>
  )
}
