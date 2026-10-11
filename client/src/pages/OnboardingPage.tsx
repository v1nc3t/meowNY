import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'

const POLICY_VERSION = '2026-10-01'

export default function OnboardingPage() {
  const navigate = useNavigate()
  const [message, setMessage] = useState('')

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const currency = String(form.get('currency')).toUpperCase()
    const response = await fetch('/api/v1/onboarding', {
      method: 'POST',
      credentials: 'include',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ currency }),
    })
    if (response.status === 409) {
      navigate('/app')
      return
    }
    if (!response.ok) {
      setMessage('could not save. use a three-letter currency code.')
      return
    }
    navigate('/app')
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>set up your account</h1>
      <p>
        accept the <Link to="/legal/terms">terms</Link> and the <Link to="/legal/privacy">privacy policy</Link> ({POLICY_VERSION}), then choose a currency. changing it later does not convert amounts.
      </p>
      <label>
        <input name="accept" type="checkbox" required /> i accept the terms and the privacy policy
      </label>
      <label>
        currency
        <input name="currency" required minLength={3} maxLength={3} pattern="[A-Za-z]{3}" placeholder="EUR" autoComplete="off" />
      </label>
      <button type="submit">continue</button>
      <p className="notice">{message}</p>
    </form>
  )
}
