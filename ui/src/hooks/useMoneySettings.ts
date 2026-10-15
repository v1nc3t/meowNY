import { useEffect, useState } from 'react'
import { errorText } from '../api/invoke'
import { vault } from '../vault/api'

export type MoneySettings = { currency: string; locale: string }

export function useMoneySettings(): { settings: MoneySettings | null; error: string } {
  const [settings, setSettings] = useState<MoneySettings | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    let cancelled = false
    vault.settings().then(
      (loaded) => {
        if (!cancelled) setSettings({ currency: loaded.currency, locale: loaded.locale })
      },
      (failure) => {
        if (!cancelled) setError(errorText(failure))
      },
    )
    return () => {
      cancelled = true
    }
  }, [])

  return { settings, error }
}
