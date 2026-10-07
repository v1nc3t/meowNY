import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { authClient } from '../auth'

export default function LoginPage() {
  const navigate = useNavigate()
  const [message, setMessage] = useState('')

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const { error } = await authClient.signIn.email({
      email: String(form.get('email')),
      password: String(form.get('password')),
    })
    if (error) {
      setMessage(error.status === 403 ? 'Verify your email first.' : 'Wrong email or password.')
      return
    }
    navigate('/app')
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>Log in</h1>
      <label>
        Email
        <input name="email" type="email" required autoComplete="email" />
      </label>
      <label>
        Password
        <input name="password" type="password" required minLength={8} autoComplete="current-password" />
      </label>
      <button type="submit">Log in</button>
      <p>{message}</p>
      <p>
        <Link to="/forgot-password">Forgot password</Link>
      </p>
    </form>
  )
}
