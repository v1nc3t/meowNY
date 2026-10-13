import { useEffect, useState, type FormEvent } from 'react'
import { messageFor } from '../api/invoke'
import { errorText, vault } from './api'
import PasswordField from './PasswordField'
import { passphraseProblem } from './rules'
import { useNotice } from '../useNotice'
import { useT } from '../i18n/context'

export default function LockScreen({ onUnlocked }: { onUnlocked: () => void }) {
  const t = useT()
  const [needsPassphrase, setNeedsPassphrase] = useState(false)
  const [keyMissing, setKeyMissing] = useState(false)
  const [recovering, setRecovering] = useState(false)
  const [message, setMessage] = useNotice()
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
        else if (error?.code === 'key_unavailable') setKeyMissing(true)
        else setMessage(errorText(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [onUnlocked, setMessage])

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
          <h1>{t('recovery.title')}</h1>
          <p className="hint">{t('recovery.enter')}</p>
          <label>
            {t('recovery.label')}
            <input
              name="recoveryKey"
              required
              autoFocus
              autoComplete="off"
              spellCheck={false}
              placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
            />
          </label>
          <PasswordField label={t('recovery.newPassphrase')} name="passphrase" />
          <PasswordField label={t('recovery.newPassphraseRepeat')} name="confirm" />
          <p className="hint">{t('recovery.emptyHint')}</p>
          <button type="submit" disabled={busy}>
            {t('common.unlock')}
          </button>
          <button
            type="button"
            onClick={() => {
              setRecovering(false)
              setMessage('')
            }}
          >
            {t('common.back')}
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
        <h1>{t('lock.title')}</h1>
        {needsPassphrase && (
          <>
            <PasswordField label={t('lock.passphrase')} name="passphrase" current autoFocus />
            <button type="submit" disabled={busy}>
              {t('common.unlock')}
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
            {keyMissing ? t('lock.useRecovery') : t('lock.forgot')}
          </button>
        )}
        {/* This one stays: it explains why the app cannot open by itself. */}
        {keyMissing ? <p className="notice">{messageFor('key_unavailable')}</p> : null}
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
    </main>
  )
}
