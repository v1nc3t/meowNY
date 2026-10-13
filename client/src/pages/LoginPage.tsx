import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { authClient, signInWithGoogle, useGoogle } from '../auth'

export default function LoginPage() {
  const navigate = useNavigate()
  const google = useGoogle()
  const [message, setMessage] = useState('')

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const { error } = await authClient.signIn.email({
      email: String(form.get('email')),
      password: String(form.get('password')),
    })
    if (error) {
      setMessage(error.status === 403 ? 'verify your email first.' : 'wrong email or password.')
      return
    }
    navigate('/app')
  }

  async function onGoogle() {
    const { error } = await signInWithGoogle()
    if (error) setMessage('google sign-in is unavailable.')
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>log in</h1>
      <label>
        email
        <input name="email" type="email" required autoComplete="email" />
      </label>
      <label>
        password
        <input name="password" type="password" required minLength={8} autoComplete="current-password" />
      </label>
      <button type="submit">log in</button>
      {google ? (
        <button type="button" onClick={() => void onGoogle()}>
          continue with google
        </button>
      ) : null}
      <p className="notice">{message}</p>
      <p>
        <Link to="/forgot-password">forgot password</Link>
      </p>
    </form>
  )
}
