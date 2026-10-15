import { useCallback, useState } from 'react'

const KEY = 'device-name'

/**
 * The name this device gives itself, which the other device shows in its list of paired devices. It is kept on this device
 * only, as a convenience; if it is lost the default comes back and the person can type it again.
 */
export function useDeviceName(fallback: string): [string, (name: string) => void] {
  const [name, setName] = useState(() => {
    try {
      return localStorage.getItem(KEY) || fallback
    } catch {
      return fallback
    }
  })
  const change = useCallback((next: string) => {
    setName(next)
    try {
      localStorage.setItem(KEY, next)
    } catch {
      // Not remembered.
    }
  }, [])
  return [name, change]
}
