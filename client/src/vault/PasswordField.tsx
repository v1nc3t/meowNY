import { useState } from 'react'

/** A password input with a show/hide button inside it, at the right edge. */
export default function PasswordField({
  label,
  name,
  current = false,
  autoFocus = false,
}: {
  label: string
  name: string
  /** True when typing an existing passphrase rather than choosing a new one. */
  current?: boolean
  autoFocus?: boolean
}) {
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
        />
        <button type="button" aria-pressed={shown} onClick={() => setShown((value) => !value)}>
          {shown ? 'hide' : 'show'}
        </button>
      </span>
    </label>
  )
}
