import { useState } from 'react'
import { useT } from '../i18n/context'

/** A password input with a show/hide button inside it, at the right edge. */
export default function PasswordField({
  label,
  name,
  current = false,
  autoFocus = false,
  onChange,
}: {
  label: string
  name: string
  /** True when typing an existing passphrase rather than choosing a new one. */
  current?: boolean
  autoFocus?: boolean
  /** Called with the text after every keystroke, for a screen that gives advice while the person types. */
  onChange?: (value: string) => void
}) {
  const t = useT()
  const [shown, setShown] = useState(false)
  return (
    <label>
      {label}
      <span className="field-input">
        <input
          name={name}
          type={shown ? 'text' : 'password'}
          autoComplete={current ? 'current-password' : 'new-password'}
          autoFocus={autoFocus}
          onChange={onChange ? (event) => onChange(event.target.value) : undefined}
        />
        <button type="button" aria-pressed={shown} onClick={() => setShown((value) => !value)}>
          {shown ? t('common.hide') : t('common.show')}
        </button>
      </span>
    </label>
  )
}
