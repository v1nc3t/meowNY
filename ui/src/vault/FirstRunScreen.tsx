import { useEffect, useRef, useState, type FormEvent } from 'react'
import { currencyOptions } from '../api/currencies'
import { cleanCategoryName } from '../api/categories'
import { saveSenderName } from '../lib/senderName'
import { formatExample, localeOptions, zoneOptions } from '../api/locales'
import Dropdown from '../components/Dropdown'
import { useI18n } from '../i18n/context'
import type { MessageKey } from '../i18n/en'
import { languageOptions } from '../i18n/languages'
import { useNotice } from '../hooks/useNotice'
import { errorText, vault } from './api'
import Decrypting from './Decrypting'
import { atLeast, closeKeyboard, useRefocus } from './unlockFlow'
import PasswordField from './PasswordField'
import RecoveryKeyPanel from './RecoveryKeyPanel'
import RestoreFromBackup from './RestoreFromBackup'
import { defaultSettings, isCurrency, normalizeCurrency, passphraseProblem } from './rules'
import Button from '../components/ui/Button'
import { Illustration, Logo, type PictureName } from '../components/ui/Pixel'

/** What the app does, on the first screen. */
const FEATURES: { picture: PictureName; title: MessageKey; text: MessageKey }[] = [
  { picture: 'spend', title: 'welcome.featSpendTitle', text: 'welcome.featSpendText' },
  { picture: 'budget', title: 'welcome.featBudgetTitle', text: 'welcome.featBudgetText' },
  { picture: 'share', title: 'welcome.featShareTitle', text: 'welcome.featShareText' },
  { picture: 'lock', title: 'welcome.featPrivateTitle', text: 'welcome.featPrivateText' },
]

/** 0 is the screen that says what the app does; 1 to 3 are the steps: about you, your money, the lock. */
type Step = 0 | 1 | 2 | 3
const STEPS = 3

/**
 * First run: what the app does, then three short steps (name and language; currency, format and time zone; the passphrase), each
 * starting from what the system suggests. The steps are one form that stays drawn, the others hidden, so what was typed survives
 * going back and forth; each step is checked when the person moves on.
 */
export default function FirstRunScreen({ onDone }: { onDone: () => void }) {
  const { t, language, setLanguage } = useI18n()
  const [message, setMessage] = useNotice()
  const [busy, setBusy] = useState(false)
  const [workDone, setWorkDone] = useState(false)
  const formRef = useRefocus(busy)
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null)
  const [restoring, setRestoring] = useState(false)
  const [suggested] = useState(defaultSettings)
  const [currency, setCurrency] = useState('')
  const [locale, setLocale] = useState(suggested.locale)
  const [timezone, setTimezone] = useState(suggested.timezone)
  const [step, setStep] = useState<Step>(0)
  // Whether the last move was back, so the new step slides in from the left rather than from the right.
  const [back, setBack] = useState(false)
  const views = useRef<(HTMLElement | null)[]>([])

  function go(next: Step) {
    setBack(next < step)
    setMessage('')
    setStep(next)
  }

  /** The cursor goes to the first field of the step, so the keyboard and a screen reader start there. */
  function focusStep(at: Step) {
    views.current[at]?.querySelector<HTMLElement>('input, .pick-button, .ui-btn')?.focus()
  }

  // Each new step takes the focus (not the first screen when the page opens).
  const shown = useRef(step)
  useEffect(() => {
    if (shown.current === step) return
    shown.current = step
    focusStep(step)
  }, [step])

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const passphrase = String(form.get('passphrase'))
    // the name friends see on the code of a shared bill: the rule of a friend's name
    const name = cleanCategoryName(String(form.get('name') ?? ''))
    // The steps up to this one, checked in order; a problem in an earlier step takes the person back to it.
    const problems: [Step, string][] = [
      [1, name ? '' : t('welcome.nameError')],
      [2, isCurrency(currency) ? '' : t('welcome.currencyError')],
      [3, passphraseProblem(passphrase, String(form.get('confirm')))],
    ]
    const wrong = problems.find(([at, problem]) => at <= step && problem)
    if (wrong || !name) {
      const [at, problem] = wrong ?? problems[0]
      if (at === step) focusStep(step)
      else go(at)
      return setMessage(problem)
    }
    if (step < STEPS) return go((step + 1) as Step)

    closeKeyboard()
    setWorkDone(false)
    setBusy(true)
    try {
      // The data file exists from here on, but the app stays on this screen until the key is saved.
      setRecoveryKey(
        await atLeast(
          vault.create({ currency: normalizeCurrency(currency), locale, timezone, language }, passphrase),
          () => setWorkDone(true),
        ),
      )
      saveSenderName(name)
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

  const slide = back ? 'is-back' : 'is-forward'
  // A step of the form: hidden (not removed) while another one shows, so its fields keep what was typed.
  const stepProps = (at: Step) => ({
    ref: (element: HTMLElement | null) => {
      views.current[at] = element
    },
    className: `onboard-step ${slide}`,
    hidden: step !== at,
    'aria-labelledby': `onboard-title-${at}`,
  })

  return (
    <main className="gate onboard">
      <section
        ref={(element) => {
          views.current[0] = element
        }}
        className={`panel onboard-welcome ${slide}`}
        hidden={step !== 0}
        aria-labelledby="onboard-title-0"
      >
        <p className="brand-mark">
          <Logo />
          meowny
        </p>
        <h1 id="onboard-title-0" className="onboard-headline">
          {t('welcome.headline')}
        </h1>
        <p className="onboard-lead">{t('welcome.intro')}</p>
        <ul className="onboard-features">
          {FEATURES.map((feature) => (
            <li key={feature.picture} className="onboard-tile">
              <Illustration name={feature.picture} scale={4} />
              <span className="onboard-tile-text">
                <b>{t(feature.title)}</b>
                <span>{t(feature.text)}</span>
              </span>
            </li>
          ))}
        </ul>
        <div className="onboard-actions">
          <Button onClick={() => go(1)}>{t('welcome.start')}</Button>
          <Button variant="outline" onClick={() => setRestoring(true)}>
            {t('welcome.restore')}
          </Button>
        </div>
      </section>

      <form ref={formRef} className="onboard-form" onSubmit={onSubmit} hidden={step === 0} noValidate>
        <p className="brand-mark">
          <Logo />
          meowny
        </p>
        <div className="onboard-progress">
          <span className="onboard-count">{t('welcome.stepOf', { step, count: STEPS })}</span>
          <span className="onboard-blocks" aria-hidden="true">
            {[1, 2, 3].map((at) => (
              <span key={at} className={at < step ? 'is-done' : at === step ? 'is-now' : undefined} />
            ))}
          </span>
        </div>

        <section {...stepProps(1)}>
          <h1 id="onboard-title-1">{t('welcome.stepYou')}</h1>
          <label>
            {t('welcome.name')}
            <input name="name" maxLength={50} autoComplete="name" aria-describedby="onboard-name-hint" />
          </label>
          <p id="onboard-name-hint" className="hint">
            {t('welcome.nameHint')}
          </p>
          <Dropdown label={t('welcome.language')} value={language} options={languageOptions} onChange={setLanguage} />
        </section>

        <section {...stepProps(2)}>
          <h1 id="onboard-title-2">{t('welcome.stepMoney')}</h1>
          <Dropdown
            label={t('welcome.currency')}
            value={currency}
            options={currencyOptions()}
            onChange={setCurrency}
            placeholder={t('welcome.currencyChoose')}
          />
          <Dropdown label={t('welcome.format')} value={locale} options={localeOptions(locale)} onChange={setLocale} />
          <p className="hint">{formatExample(locale, currency || 'EUR')}</p>
          <Dropdown label={t('welcome.timezone')} value={timezone} options={zoneOptions(timezone)} onChange={setTimezone} />
        </section>

        <section {...stepProps(3)}>
          <h1 id="onboard-title-3">{t('welcome.stepLock')}</h1>
          <p className="hint">{t('welcome.lockHint')}</p>
          <PasswordField label={t('welcome.passphrase')} name="passphrase" />
          <PasswordField label={t('welcome.passphraseRepeat')} name="confirm" />
          <p className="warning" role="note">
            {t('welcome.warning')}
          </p>
        </section>

        <p className="notice" role="alert">
          {message}
        </p>
        <div className="onboard-nav">
          <Button variant="outline" disabled={busy} onClick={() => go((step - 1) as Step)}>
            {t('common.back')}
          </Button>
          <Button type="submit" disabled={busy}>
            {step === STEPS ? t('welcome.create') : t('welcome.next')}
          </Button>
        </div>
      </form>
      {busy && <Decrypting below creating done={workDone} />}
    </main>
  )
}
