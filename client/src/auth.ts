import { useEffect, useState } from 'react'
import { createAuthClient } from 'better-auth/react'

export const authClient = createAuthClient()

export function signInWithGoogle() {
  return authClient.signIn.social({
    provider: 'google',
    callbackURL: `${window.location.origin}/app`,
  })
}

export function useGoogle(): boolean {
  const [enabled, setEnabled] = useState(false)
  useEffect(() => {
    let cancelled = false
    fetch('/api/auth')
      .then(async (response) => (response.ok ? response.json() : { providers: [] }))
      .then((body: { providers?: string[] }) => {
        if (!cancelled) setEnabled(body.providers?.includes('google') ?? false)
      })
      .catch(() => {
        if (!cancelled) setEnabled(false)
      })
    return () => {
      cancelled = true
    }
  }, [])
  return enabled
}
