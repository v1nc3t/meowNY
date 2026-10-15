import { useCallback, useEffect, useRef, useState } from 'react'
import { isBusy, POLL_MS, syncApi, type Status } from '../api/sync'

/**
 * The status of a pairing or a sync, kept up to date while something runs. Call `refresh` right after starting one.
 * Leaving the screen while a code is on show ends it, so no window stays open that nobody can see.
 */
export function useSyncStatus(): {
  status: Status
  refresh: () => Promise<void>
  cancel: () => Promise<void>
  acknowledge: () => Promise<void>
} {
  const [status, setStatus] = useState<Status>({ state: 'idle' })
  const latest = useRef(status)
  useEffect(() => {
    latest.current = status
  })

  const refresh = useCallback(async () => {
    try {
      setStatus(await syncApi.status())
    } catch {
      // The next poll tries again.
    }
  }, [])

  // A run may already be going on (the person came back to this screen).
  useEffect(() => {
    let stale = false
    syncApi.status().then(
      (current) => {
        if (!stale) setStatus(current)
      },
      () => undefined,
    )
    return () => {
      stale = true
    }
  }, [])

  const busy = isBusy(status)
  useEffect(() => {
    if (!busy) return
    const timer = setInterval(() => void refresh(), POLL_MS)
    return () => clearInterval(timer)
  }, [busy, refresh])

  useEffect(
    () => () => {
      const { state } = latest.current
      if (state === 'waiting' || state === 'checkCode') void syncApi.cancel().catch(() => undefined)
    },
    [],
  )

  const cancel = useCallback(async () => {
    try {
      await syncApi.cancel()
    } catch {
      // Nothing was running.
    }
    await refresh()
  }, [refresh])

  const acknowledge = useCallback(async () => {
    try {
      await syncApi.acknowledge()
    } catch {
      // The status is read again below either way.
    }
    await refresh()
  }, [refresh])

  return { status, refresh, cancel, acknowledge }
}
