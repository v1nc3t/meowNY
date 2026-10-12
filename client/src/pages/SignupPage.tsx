import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { authClient, signInWithGoogle, useGoogle } from '../auth'

export default function SignupPage() {
  const google = useGoogle()
  const [message, setMessage] = useState('')

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const { error } = await authClient.signUp.email({
      name: String(form.get('name')),
      email: String(form.get('email')),
      password: String(form.get('password')),
      callbackURL: `${window.location.origin}/login`,
    })
    setMessage(
      error
        ? 'could not sign up.'
        : `check your email for a verification link.${import.meta.env.DEV ? ' in development it arrives in the local inbox at http://127.0.0.1:8025.' : ''}`,
    )
  }

  async function onGoogle() {
    const { error } = await signInWithGoogle()
    if (error) setMessage('google sign-in is unavailable.')
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>sign up</h1>
      <label>
        name
        <input name="name" required autoComplete="name" />
      </label>
      <label>
        email
        <input name="email" type="email" required autoComplete="email" />
      </label>
      <label>
        password
        <input name="password" type="password" required minLength={8} autoComplete="new-password" />
      </label>
      <button type="submit">sign up</button>
      {google ? (
        <button type="button" onClick={() => void onGoogle()}>
          continue with google
        </button>
      ) : null}
      <p className="notice">{message}</p>
      <p>
        <Link to="/login">log in</Link>
      </p>
    </form>
  )
}
