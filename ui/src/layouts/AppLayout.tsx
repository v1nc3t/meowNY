import { Fragment, Suspense, useCallback, useEffect, useMemo, useRef, useState, type TouchEvent } from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'
import PhoneTabs, { PHONE_TABS } from '../components/PhoneTabs'
import QuickAdd from '../components/QuickAdd'
import Toaster from '../components/Toaster'
import { preloadAllWhenIdle } from '../pages'
import { DataVersionContext } from '../hooks/dataVersion'
import { blocksSwipe, SwipeContext, type SwipeHandler } from '../hooks/swipe'
import { useDebtReminders } from '../hooks/useDebtReminders'
import { useDueNotifications } from '../hooks/useDueNotifications'
import { useProfile } from '../hooks/profile'
import { neighbour, swipeDirection, type Direction } from '../lib/swipe'
import ShortcutsProvider from '../keys/ShortcutsProvider'

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('sidebar-collapsed') === '1')
  const [dataVersion, setDataVersion] = useState(0)
  const locked = true
  useDueNotifications()
  useDebtReminders()
  const phone = useProfile() === 'phone'
  const navigate = useNavigate()
  const { pathname } = useLocation()
  // Pages that have sections of their own (the budgets) answer a swipe first; the newest one is asked first.
  const handlers = useRef<SwipeHandler[]>([])
  const touch = useRef<{ x: number; y: number } | null>(null)
  const [direction, setDirection] = useState<Direction | null>(null)

  useEffect(() => {
    preloadAllWhenIdle()
  }, [])

  useEffect(() => {
    if (!direction) return
    const timer = setTimeout(() => setDirection(null), 300)
    return () => clearTimeout(timer)
  }, [direction])

  const register = useCallback((handler: SwipeHandler) => {
    handlers.current.push(handler)
    return () => {
      handlers.current = handlers.current.filter((other) => other !== handler)
    }
  }, [])
  const swipes = useMemo(() => ({ register }), [register])

  function onTouchStart(event: TouchEvent) {
    const finger = event.touches[0]
    touch.current = event.touches.length === 1 && finger && !blocksSwipe(event.target as Element) ? { x: finger.clientX, y: finger.clientY } : null
  }

  function onTouchEnd(event: TouchEvent) {
    const start = touch.current
    touch.current = null
    const finger = event.changedTouches[0]
    if (!start || !finger) return
    const way = swipeDirection(finger.clientX - start.x, finger.clientY - start.y, start.x, window.innerWidth)
    if (!way) return
    if (handlers.current[handlers.current.length - 1]?.(way)) return
    const to = neighbour(PHONE_TABS.map((tab) => tab.to), pathname, way)
    if (to) {
      setDirection(way)
      navigate(to)
    }
  }

  const toggleSidebar = useCallback(() => {
    setCollapsed((current) => {
      const next = !current
      localStorage.setItem('sidebar-collapsed', next ? '1' : '0')
      return next
    })
  }, [])

  const dataChanges = useMemo(() => ({ version: dataVersion, changed: () => setDataVersion((current) => current + 1) }), [dataVersion])

  const shell = ['app-shell', collapsed ? 'is-collapsed' : '', locked ? 'is-locked' : '', phone ? 'is-phone' : ''].filter(Boolean).join(' ')

  return (
    <DataVersionContext.Provider value={dataChanges}>
      <SwipeContext.Provider value={swipes}>
        <ShortcutsProvider onToggleSidebar={toggleSidebar} enabled={!phone}>
          <div className={shell}>
            <AppSidebar collapsed={collapsed} onToggle={toggleSidebar} />
            <div className="app-main" onTouchStart={phone ? onTouchStart : undefined} onTouchEnd={phone ? onTouchEnd : undefined}>
              <main data-direction={direction ?? undefined}>
                <Suspense fallback={null}>
                  {/* A new key draws the page again, so it shows a transaction that the quick add just made. */}
                  <Fragment key={dataVersion}>
                    <Outlet />
                  </Fragment>
                </Suspense>
              </main>
            </div>
            {phone ? (
              <>
                <PhoneTabs />
                <QuickAdd />
              </>
            ) : null}
          </div>
          <Toaster />
        </ShortcutsProvider>
      </SwipeContext.Provider>
    </DataVersionContext.Provider>
  )
}
