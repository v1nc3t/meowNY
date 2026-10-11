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
    setMessage(error ? 'could not reset the password.' : 'password updated. you can log in.')
  }

  if (!token) return <p className="notice">this reset link is invalid.</p>

  return (
    <form onSubmit={onSubmit}>
      <h1>reset password</h1>
      <label>
        new password
        <input name="password" type="password" required minLength={8} autoComplete="new-password" />
      </label>
      <button type="submit">save</button>
      <p className="notice">{message}</p>
      <p>
        <Link to="/login">log in</Link>
      </p>
    </form>
  )
}
