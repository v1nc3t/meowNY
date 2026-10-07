import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { authClient } from '../auth'

export default function SignupPage() {
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
    setMessage(error ? 'Could not sign up.' : 'Check your email for a verification link.')
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>Sign up</h1>
      <label>
        Name
        <input name="name" required autoComplete="name" />
      </label>
      <label>
        Email
        <input name="email" type="email" required autoComplete="email" />
      </label>
      <label>
        Password
        <input name="password" type="password" required minLength={8} autoComplete="new-password" />
      </label>
      <button type="submit">Sign up</button>
      <p>{message}</p>
      <p>
        <Link to="/login">Log in</Link>
      </p>
    </form>
  )
}
