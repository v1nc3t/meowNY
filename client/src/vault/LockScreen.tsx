import { useEffect, useState, type FormEvent } from 'react'
import { errorText, vault } from './api'
import PasswordField from './PasswordField'
import { passphraseProblem } from './rules'

export default function LockScreen({ onUnlocked }: { onUnlocked: () => void }) {
  const [needsPassphrase, setNeedsPassphrase] = useState(false)
  const [keyMissing, setKeyMissing] = useState(false)
  const [recovering, setRecovering] = useState(false)
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
        else {
          if (error?.code === 'key_unavailable') setKeyMissing(true)
          setMessage(errorText(error))
        }
      },
    )
    return () => {
      cancelled = true
    }
  }, [onUnlocked])

  async function onUnlock(event: FormEvent<HTMLFormElement>) {
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

  async function onRecover(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const passphrase = String(form.get('passphrase'))
    const problem = passphraseProblem(passphrase, String(form.get('confirm')))
    if (problem) return setMessage(problem)
    setBusy(true)
    try {
      await vault.recover(String(form.get('recoveryKey')), passphrase)
      onUnlocked()
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  if (recovering) {
    return (
      <main className="gate">
        <form onSubmit={onRecover}>
          <p className="brand-mark">meowny</p>
          <h1>recovery key</h1>
          <p className="hint">enter the recovery key you saved when you set up. then choose a new passphrase.</p>
          <label>
            recovery key
            <input
              name="recoveryKey"
              required
              autoFocus
              autoComplete="off"
              spellCheck={false}
              placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
            />
          </label>
          <PasswordField label="new passphrase (optional)" name="passphrase" />
          <PasswordField label="repeat new passphrase" name="confirm" />
          <p className="hint">leave the passphrase empty to open the app without one.</p>
          <button type="submit" disabled={busy}>
            unlock
          </button>
          <button
            type="button"
            onClick={() => {
              setRecovering(false)
              setMessage('')
            }}
          >
            back
          </button>
          <p className="notice" role="alert">
            {message}
          </p>
        </form>
      </main>
    )
  }

  return (
    <main className="gate">
      <form onSubmit={onUnlock}>
        <p className="brand-mark">meowny</p>
        <h1>locked</h1>
        {needsPassphrase && (
          <>
            <PasswordField label="passphrase" name="passphrase" current autoFocus />
            <button type="submit" disabled={busy}>
              unlock
            </button>
          </>
        )}
        {(needsPassphrase || keyMissing) && (
          <button
            type="button"
            onClick={() => {
              setRecovering(true)
              setMessage('')
            }}
          >
            {keyMissing ? 'use my recovery key' : 'forgot my passphrase'}
          </button>
        )}
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
    </main>
  )
}
