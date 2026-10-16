import { useEffect, useState } from 'react'

/** A value that follows `value` after it has stopped changing for `ms`. Used so typing does not query on every key. */
export function useDebounced<T>(value: T, ms = 200): T {
  const [settled, setSettled] = useState(value)
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms)
    return () => clearTimeout(timer)
  }, [value, ms])
  return settled
}
