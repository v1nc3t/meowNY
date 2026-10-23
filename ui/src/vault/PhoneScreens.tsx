import { useState, type FormEvent } from 'react'
import { PHONE_PLACEHOLDER_CURRENCY, wantsLongerPassphrase } from '../api/sync'
import Dropdown from '../components/Dropdown'
import { useNotice } from '../hooks/useNotice'
import { useI18n, useT } from '../i18n/context'
import { languageOptions } from '../i18n/languages'
import { errorText, vault } from './api'
import Decrypting from './Decrypting'
import { atLeast, closeKeyboard, useRefocus } from './decrypting'
import PasswordField from './PasswordField'
import { defaultSettings, phonePassphraseProblem } from './rules'
import Button from '../components/ui/Button'

/**
 * First start on a phone: a language and a passphrase, nothing else. The phone is a copy of the desktop, so its currency, number
 * format and time zone come from the desktop with the first pairing (the placeholder currency is replaced, not converted), and
 * there is no recovery key: if the passphrase is lost, the phone is erased and paired again.
 */
export function PhoneFirstRun({ onDone }: { onDone: () => void }) {
  const { t, language, setLanguage } = useI18n()
  const [message, setMessage] = useNotice()
  const [busy, setBusy] = useState(false)
  const [typed, setTyped] = useState('')
  const formRef = useRefocus(busy)

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const passphrase = String(form.get('passphrase'))
    const problem = phonePassphraseProblem(passphrase, String(form.get('confirm')))
    if (problem) return setMessage(problem)
    closeKeyboard()
    setBusy(true)
    try {
      const { locale, timezone } = defaultSettings()
      // The recovery key is not shown: on a phone it could only open the data, and the data is on the desktop too.
      await atLeast(vault.create({ currency: PHONE_PLACEHOLDER_CURRENCY, locale, timezone, language }, passphrase))
      onDone()
    } catch (error) {
      setMessage(errorText(error))
      setBusy(false)
    }
  }

  return (
    <main className="gate">
      <form ref={formRef} onSubmit={onSubmit}>
        <p className="brand-mark">meowny</p>
        <h1>{t('phone.welcomeTitle')}</h1>

        <Dropdown label={t('welcome.language')} value={language} options={languageOptions} onChange={setLanguage} />

        <PasswordField label={t('phone.passphrase')} name="passphrase" onChange={setTyped} />
        <PasswordField label={t('phone.passphraseRepeat')} name="confirm" />
        {wantsLongerPassphrase(typed) ? (
          <p className="warning" role="note">
            {t('phone.passphraseLonger')}
          </p>
        ) : null}

        <p className="warning" role="note">
          {t('phone.eraseWarning')}
        </p>

        <Button type="submit" disabled={busy}>
          {t('phone.welcomeCreate')}
        </Button>
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
      {busy && <Decrypting below />}
    </main>
  )
}

/** After the 5th wrong passphrase: says what happened and what to do, then goes on to the first start again. */
export function ErasedScreen({ onContinue }: { onContinue: () => void }) {
  const t = useT()
  return (
    <main className="gate">
      <div className="panel">
        <p className="brand-mark">meowny</p>
        <h1>{t('phone.erasedTitle')}</h1>
        <p className="warning" role="alert">
          {t('phone.erasedText')}
        </p>
        <p className="hint">{t('phone.erasedNext')}</p>
        <Button onClick={onContinue}>{t('common.continue')}</Button>
      </div>
    </main>
  )
}

/** "Forgot my passphrase" on a phone: there is nothing to recover with, so the way forward is to erase it and pair it again. */
export function ForgotPhone({ onBack }: { onBack: () => void }) {
  const t = useT()
  const [word, setWord] = useState('')
  const [message, setMessage] = useNotice()

  async function onErase(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    try {
      await vault.wipe(word)
      window.location.assign('/')
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  return (
    <main className="gate">
      <form onSubmit={onErase}>
        <p className="brand-mark">meowny</p>
        <h1>{t('phone.forgotTitle')}</h1>
        <p className="hint">{t('phone.forgotText')}</p>
        <p className="warning" role="note">
          {t('phone.forgotWarning')}
        </p>
        <label>
          {t('settings.deleteConfirm')}
          <input value={word} onChange={(e) => setWord(e.target.value)} autoComplete="off" />
        </label>
        <Button type="submit" variant="destructive" disabled={word !== 'DELETE'}>
          {t('phone.forgotErase')}
        </Button>
        <Button variant="outline" onClick={onBack}>
          {t('common.back')}
        </Button>
        <p className="notice" role="alert">
          {message}
        </p>
      </form>
    </main>
  )
}
