import { useState, type FormEvent } from 'react'
import { useI18n } from '../i18n/context'
import { useNotice } from '../hooks/useNotice'
import { errorText, vault } from './api'
import PasswordField from './PasswordField'
import { passphraseProblem } from './rules'

/** First run from a backup: the backup's passphrase, and the app passphrase for this new install. */
export default function RestoreFromBackup({ onKey, onBack }: { onKey: (recoveryKey: string) => void; onBack: () => void }) {
  const { t } = useI18n()
  const [message, setMessage] = useNotice()
  const [busy, setBusy] = useState(false)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const passphrase = String(form.get('passphrase'))
    const problem = passphraseProblem(passphrase, String(form.get('confirm')))
    if (problem) return setMessage(problem)
    setBusy(true)
    try {
      const key = await vault.restoreFromBackup(String(form.get('backup_passphrase')), passphrase)
      if (key) onKey(key)
    } catch (error) {
      setMessage(errorText(error))
    }
    setBusy(false)
  }

  return (
    <main className="gate">
      <form onSubmit={onSubmit}>
        <p className="brand-mark">meowny</p>
        <h1>{t('welcome.restore')}</h1>
        <p className="hint">{t('welcome.restoreIntro')}</p>
        <PasswordField label={t('settings.backupPassphrase')} name="backup_passphrase" current autoFocus />
        <PasswordField label={t('welcome.passphrase')} name="passphrase" />
        <PasswordField label={t('welcome.passphraseRepeat')} name="confirm" />
        <p className="hint">{t('welcome.passphraseHint')}</p>
        <button type="submit" disabled={busy}>
          {t('welcome.restoreSubmit')}
        </button>
        <button type="button" onClick={onBack}>
          {t('common.back')}
        </button>
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
    </main>
  )
}
