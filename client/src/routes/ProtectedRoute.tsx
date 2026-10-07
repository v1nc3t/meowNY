import { useEffect, useState } from 'react'
import { Navigate, Outlet, useLocation } from 'react-router-dom'
import { authClient } from '../auth'

type Gate = 'session' | 'settings' | 'ready'

export default function ProtectedRoute() {
  const location = useLocation()
  const { data: session, isPending } = authClient.useSession()
  const userId = session?.user.id
  const [gate, setGate] = useState<Gate>('session')
  const [hasSettings, setHasSettings] = useState(false)
  const onboarding = location.pathname === '/onboarding'

  useEffect(() => {
    if (isPending || !userId) return
    let cancelled = false
    fetch('/api/v1/me', { credentials: 'include' })
      .then(async (response) => {
        if (!response.ok) throw new Error('me')
        return response.json() as Promise<{ settings: unknown }>
      })
      .then((body) => {
        if (cancelled) return
        setHasSettings(body.settings !== null)
        setGate('ready')
      })
      .catch(() => {
        if (!cancelled) setGate('settings')
      })
    return () => {
      cancelled = true
    }
  }, [isPending, userId])

  if (isPending || (session && gate === 'session')) return null
  if (!session || gate === 'settings') return <Navigate to="/login" replace />
  if (!hasSettings && !onboarding) return <Navigate to="/onboarding" replace />
  if (hasSettings && onboarding) return <Navigate to="/app" replace />
  return <Outlet />
}