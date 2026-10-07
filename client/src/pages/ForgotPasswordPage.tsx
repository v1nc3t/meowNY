import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { authClient } from '../auth'

export default function ForgotPasswordPage() {
  const [message, setMessage] = useState('')

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const { error } = await authClient.requestPasswordReset({
      email: String(form.get('email')),
      redirectTo: `${window.location.origin}/reset-password`,
    })
    setMessage(error ? 'Could not send the reset link.' : 'If that email is registered, a reset link is on its way.')
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>Forgot password</h1>
      <label>
        Email
        <input name="email" type="email" required autoComplete="email" />
      </label>
      <button type="submit">Send reset link</button>
      <p>{message}</p>
      <p>
        <Link to="/login">Log in</Link>
      </p>
    </form>
  )
}
