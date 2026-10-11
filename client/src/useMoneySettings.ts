import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api } from './http'

export type MoneySettings = { currency: string; locale: string }

export function useMoneySettings(): { settings: MoneySettings | null; error: string } {
  const navigate = useNavigate()
  const [settings, setSettings] = useState<MoneySettings | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    let cancelled = false
    void api('/api/v1/me')
      .then(async (response) => {
        if (cancelled) return
        if (!response.ok) {
          setError('could not load your settings.')
          return
        }
        const body = (await response.json()) as { settings: MoneySettings | null }
        if (cancelled) return
        if (!body.settings) {
          navigate('/onboarding')
          return
        }
        setSettings({ currency: body.settings.currency, locale: body.settings.locale })
      })
      .catch(() => {
        if (!cancelled) setError('could not load your settings.')
      })
    return () => {
      cancelled = true
    }
  }, [navigate])

  return { settings, error }
}
