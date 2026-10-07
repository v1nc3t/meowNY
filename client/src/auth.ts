import { createAuthClient } from 'better-auth/react'

export const authClient = createAuthClient()

export function signInWithGoogle() {
  return authClient.signIn.social({
    provider: 'google',
    callbackURL: `${window.location.origin}/app`,
  })
}
