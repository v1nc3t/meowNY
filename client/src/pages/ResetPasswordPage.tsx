import { useState, type FormEvent } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { authClient } from '../auth'

export default function ResetPasswordPage() {
  const [params] = useSearchParams()
  const token = params.get('token')
  const [message, setMessage] = useState('')

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!token) return
    const password = String(new FormData(event.currentTarget).get('password'))
    const { error } = await authClient.resetPassword({ newPassword: password, token })
    setMessage(error ? 'Could not reset the password.' : 'Password updated. You can log in.')
  }

  if (!token) return <p>This reset link is invalid.</p>

  return (
    <form onSubmit={onSubmit}>
      <h1>Reset password</h1>
      <label>
        New password
        <input name="password" type="password" required minLength={8} autoComplete="new-password" />
      </label>
      <button type="submit">Save</button>
      <p>{message}</p>
      <p>
        <Link to="/login">Log in</Link>
      </p>
    </form>
  )
}
