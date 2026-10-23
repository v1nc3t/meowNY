import { useEffect, useState, type FormEvent } from 'react'
import { messageFor } from '../api/invoke'
import { shouldWarn, syncApi, type Attempts } from '../api/sync'
import { errorText, vault, VaultError } from './api'
import Decrypting, { DecryptingPage } from './Decrypting'
import { atLeast, useDecryptingGate, useRefocus } from './decrypting'
import PasswordField from './PasswordField'
import { ErasedScreen, ForgotPhone } from './PhoneScreens'
import { passphraseProblem } from './rules'
import { useNotice } from '../hooks/useNotice'
import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import Button from '../components/ui/Button'

export default function LockScreen({ onUnlocked }: { onUnlocked: () => void }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const [attempts, setAttempts] = useState<Attempts | null>(null)
  const [erased, setErased] = useState(false)
  const [needsPassphrase, setNeedsPassphrase] = useState(phone)
  const [keyMissing, setKeyMissing] = useState(false)
  const [recovering, setRecovering] = useState(false)
  const [message, setMessage] = useNotice()
  const [busy, setBusy] = useState(false)
  // The silent first try (desktop, keychain only) is a loading state of its own; `opened` waits for the loading screen to finish.
  const [trying, setTrying] = useState(!phone)
  const [opened, setOpened] = useState(false)
  const loading = useDecryptingGate(trying)
  const formRef = useRefocus(busy)

  // Try without a passphrase first: a keychain-only vault opens straight away. A phone always asks.
  useEffect(() => {
    if (phone) {
      syncApi.attempts().then(setAttempts, () => undefined)
      return
    }
    let cancelled = false
    vault.unlock('').then(
      () => {
        if (cancelled) return
        setTrying(false)
        setOpened(true)
      },
      (error) => {
        if (cancelled) return
        setTrying(false)
        if (error?.code === 'wrong_credentials') setNeedsPassphrase(true)
        else if (error?.code === 'key_unavailable') setKeyMissing(true)
        else setMessage(errorText(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [phone, setMessage])

  // Go on into the app once the data is open and the loading screen (if it came up) has had its time.
  useEffect(() => {
    if (opened && !loading.show) onUnlocked()
  }, [opened, loading.show, onUnlocked])

  async function onUnlock(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const passphrase = String(new FormData(event.currentTarget).get('passphrase'))
    setBusy(true)
    try {
      await atLeast(vault.unlock(passphrase))
      onUnlocked()
    } catch (error) {
      if (error instanceof VaultError && error.code === 'erased') return setErased(true)
      setMessage(errorText(error))
      setBusy(false)
      if (phone) syncApi.attempts().then(setAttempts, () => undefined)
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
      await atLeast(vault.recover(String(form.get('recoveryKey')), passphrase))
      onUnlocked()
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  if (loading.show) return <DecryptingPage />
  if (loading.blank || opened) return null

  if (erased) return <ErasedScreen onContinue={onUnlocked} />

  if (recovering && phone) return <ForgotPhone onBack={() => setRecovering(false)} />

  if (recovering) {
    return (
      <main className="gate">
        {busy && <Decrypting />}
        <form ref={formRef} hidden={busy} onSubmit={onRecover}>
          <p className="brand-mark">meowny</p>
          <h1>{t('recovery.title')}</h1>
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
          <Button type="submit" disabled={busy}>
            {t('common.unlock')}
          </Button>
          <Button
            variant="outline"
            onClick={() => {
              setRecovering(false)
              setMessage('')
            }}
          >
            {t('common.back')}
          </Button>
          <p className="notice" role="alert">
            {message}
          </p>
        </form>
      </main>
    )
  }

  return (
    <main className="gate">
      {busy && <Decrypting />}
      <form ref={formRef} hidden={busy} onSubmit={onUnlock}>
        <p className="brand-mark">meowny</p>
        <h1>{t('lock.title')}</h1>
        {needsPassphrase && (
          <>
            <PasswordField label={t('lock.passphrase')} name="passphrase" current autoFocus />
            {phone && shouldWarn(attempts) && attempts ? (
              <p className="warning" role="alert">
                {t(attempts.left === 1 ? 'lock.lastAttempt' : 'lock.attemptsLeft', { count: attempts.left })}
              </p>
            ) : null}
            <Button type="submit" disabled={busy}>
              {t('common.unlock')}
            </Button>
          </>
        )}
        {(needsPassphrase || keyMissing) && (
          <Button
            variant={needsPassphrase ? 'ghost' : 'default'}
            onClick={() => {
              setRecovering(true)
              setMessage('')
            }}
          >
            {keyMissing && !phone ? t('lock.useRecovery') : t('lock.forgot')}
          </Button>
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
