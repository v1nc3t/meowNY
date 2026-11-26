import { useCallback, useRef, useState } from 'react'
import { defaultLayout, type Layout } from '../lib/dashboardLayout'
import { clearLayout, loadLayout, saveLayout } from '../lib/dashboardStorage'
import type { Profile } from '../lib/dashboardWidgets'

let counter = 0

/** An id for a new widget that no widget of `layout` has. */
export function newWidgetId(layout: Layout): string {
  for (;;) {
    const id = `w${Date.now().toString(36)}${(counter++).toString(36)}`
    if (!layout.widgets.some((widget) => widget.id === id)) return id
  }
}

/**
 * The dashboard's layout of this device: read from the storage at the start, and kept again at every change. `update` takes a function from
 * the layout to the next one (the pure functions of lib/dashboardLayout.ts); `reset` brings the default back and forgets the stored one.
 */
export function useDashboardLayout(profile: Profile) {
  const [layout, setLayout] = useState(() => loadLayout(profile))
  const latest = useRef(layout)

  const update = useCallback(
    (change: (layout: Layout) => Layout) => {
      const next = change(latest.current)
      if (next === latest.current) return
      latest.current = next
      setLayout(next)
      saveLayout(profile, next)
    },
    [profile],
  )

  const reset = useCallback(() => {
    latest.current = defaultLayout(profile)
    setLayout(latest.current)
    clearLayout(profile)
  }, [profile])

  return { layout, update, reset }
}
