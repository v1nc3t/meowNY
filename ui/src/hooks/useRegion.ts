import { useEffect, useState } from 'react'
import { vault } from '../vault/api'

/** The number format and time zone of the data, for showing times. Until they are known, the browser's own. */
export function useRegion(): { locale: string; timezone: string } {
  const [region, setRegion] = useState(() => ({
    locale: typeof navigator === 'undefined' ? 'en' : navigator.language,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
  }))
  useEffect(() => {
    let cancelled = false
    vault.settings().then(
      (saved) => {
        if (!cancelled) setRegion({ locale: saved.locale, timezone: saved.timezone })
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [])
  return region
}
