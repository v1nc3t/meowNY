import { Suspense, useCallback, useEffect, useId, useMemo, useRef, useState, type TouchEvent } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'
import PhoneMenu from '../components/PhoneMenu'
import QuickAdd from '../components/QuickAdd'
import Toaster from '../components/Toaster'
import { preloadAllWhenIdle } from '../pages'
import { blocksSwipe, SwipeContext, type SwipeHandler } from '../hooks/swipe'
import { PhoneMenuContext } from '../hooks/phoneMenu'
import { useDebtReminders } from '../hooks/useDebtReminders'
import { useDueNotifications } from '../hooks/useDueNotifications'
import { useProfile } from '../hooks/profile'
import { opensMenu, swipeDirection } from '../lib/swipe'
import ShortcutsProvider from '../keys/ShortcutsProvider'

// The round add button has nothing to add on these pages of a phone.
const NO_ADD = ['/app/settings', '/app/sync']

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('sidebar-collapsed') === '1')
  useDueNotifications()
  useDebtReminders()
  const phone = useProfile() === 'phone'
  const { pathname } = useLocation()
  // Pages that have sections of their own (the budgets) answer a swipe first; the newest one is asked first.
  const handlers = useRef<SwipeHandler[]>([])
  const touch = useRef<{ x: number; y: number } | null>(null)
  // The phone's menu: the logo button of each page's header opens it (`usePhoneMenu`), and so does a swipe from the left edge.
  const [menuOpen, setMenuOpen] = useState(false)
  const menuId = useId()
  const menu = useMemo(() => ({ open: () => setMenuOpen(true), isOpen: menuOpen, id: menuId }), [menuOpen, menuId])
  const closeMenu = useCallback(() => setMenuOpen(false), [])

  useEffect(() => {
    preloadAllWhenIdle()
  }, [])

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
    const dx = finger.clientX - start.x
    const dy = finger.clientY - start.y
    if (opensMenu(dx, dy, start.x)) {
      setMenuOpen(true)
      return
    }
    const way = swipeDirection(dx, dy, start.x, window.innerWidth)
    if (way) handlers.current[handlers.current.length - 1]?.(way)
  }

  const toggleSidebar = useCallback(() => {
    setCollapsed((current) => {
      const next = !current
      localStorage.setItem('sidebar-collapsed', next ? '1' : '0')
      return next
    })
  }, [])

  const shell = ['app-shell', collapsed ? 'is-collapsed' : '', 'is-locked', phone ? 'is-phone' : ''].filter(Boolean).join(' ')

  return (
    <SwipeContext.Provider value={swipes}>
      <PhoneMenuContext.Provider value={menu}>
        <ShortcutsProvider onToggleSidebar={toggleSidebar} enabled={!phone}>
          <div className={shell}>
            <AppSidebar collapsed={collapsed} onToggle={toggleSidebar} />
            <div className="app-main" onTouchStart={phone ? onTouchStart : undefined} onTouchEnd={phone ? onTouchEnd : undefined}>
              <main>
                <Suspense fallback={null}>
                  <Outlet />
                </Suspense>
              </main>
            </div>
            {phone && !NO_ADD.includes(pathname) ? <QuickAdd /> : null}
          </div>
          {phone && menuOpen ? <PhoneMenu id={menuId} onClose={closeMenu} /> : null}
          <Toaster />
        </ShortcutsProvider>
      </PhoneMenuContext.Provider>
    </SwipeContext.Provider>
  )
}
