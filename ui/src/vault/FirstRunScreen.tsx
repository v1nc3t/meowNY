import { useState, type FormEvent } from 'react'
import { currencyOptions } from '../api/currencies'
import { formatExample, localeOptions, zoneOptions } from '../api/locales'
import Dropdown from '../components/Dropdown'
import { useI18n } from '../i18n/context'
import { languageOptions } from '../i18n/languages'
import { useNotice } from '../hooks/useNotice'
import { errorText, vault } from './api'
import PasswordField from './PasswordField'
import RecoveryKeyPanel from './RecoveryKeyPanel'
import RestoreFromBackup from './RestoreFromBackup'
import { defaultSettings, isCurrency, normalizeCurrency, passphraseProblem } from './rules'

/** First run: language, region, currency and security, all in one place, each starting from what the system suggests. */
export default function FirstRunScreen({ onDone }: { onDone: () => void }) {
  const { t, language, setLanguage } = useI18n()
  const [message, setMessage] = useNotice()
  const [busy, setBusy] = useState(false)
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null)
  const [restoring, setRestoring] = useState(false)
  const [suggested] = useState(defaultSettings)
  const [currency, setCurrency] = useState('')
  const [locale, setLocale] = useState(suggested.locale)
  const [timezone, setTimezone] = useState(suggested.timezone)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const passphrase = String(form.get('passphrase'))
    const problem = passphraseProblem(passphrase, String(form.get('confirm')))
    if (!isCurrency(currency)) return setMessage(t('welcome.currencyError'))
    if (problem) return setMessage(problem)

    setBusy(true)
    try {
      // The data file exists from here on, but the app stays on this screen until the key is saved.
      setRecoveryKey(
        await vault.create({ currency: normalizeCurrency(currency), locale, timezone, language }, passphrase),
      )
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  if (recoveryKey) {
    return (
      <main className="gate">
        <RecoveryKeyPanel code={recoveryKey} confirm doneLabel={t('common.continue')} onDone={onDone} />
      </main>
    )
  }

  if (restoring) return <RestoreFromBackup onKey={setRecoveryKey} onBack={() => setRestoring(false)} />

  return (
    <main className="gate">
      <form onSubmit={onSubmit}>
        <p className="brand-mark">meowny</p>
        <h1>{t('welcome.title')}</h1>

        <Dropdown
          label={t('welcome.language')}
          value={language}
          options={languageOptions}
          onChange={setLanguage}
        />

        <Dropdown
          label={t('welcome.format')}
          value={locale}
          options={localeOptions(locale)}
          onChange={setLocale}
        />
        <p className="hint">{formatExample(locale, currency || 'EUR')}</p>

        <Dropdown label={t('welcome.timezone')} value={timezone} options={zoneOptions(timezone)} onChange={setTimezone} />

        <Dropdown
          label={t('welcome.currency')}
          value={currency}
          options={currencyOptions()}
          onChange={setCurrency}
          placeholder={t('welcome.currencyChoose')}
        />

        <PasswordField label={t('welcome.passphrase')} name="passphrase" />
        <PasswordField label={t('welcome.passphraseRepeat')} name="confirm" />

        <p className="warning" role="note">
          {t('welcome.warning')}
        </p>

        <button type="submit" disabled={busy}>
          {t('welcome.create')}
        </button>
        <button type="button" onClick={() => setRestoring(true)}>
          {t('welcome.restore')}
        </button>
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
    </main>
  )
}
