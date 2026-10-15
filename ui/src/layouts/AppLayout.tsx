import { Suspense, useCallback, useEffect, useState } from 'react'
import { Outlet } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'
import { preloadAllWhenIdle } from '../pages'
import { useDueNotifications } from '../hooks/useDueNotifications'
import { useProfile } from '../hooks/profile'
import ShortcutsProvider from '../keys/ShortcutsProvider'

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('sidebar-collapsed') === '1')
  const locked = true
  useDueNotifications()
  const phone = useProfile() === 'phone'

  useEffect(() => {
    preloadAllWhenIdle()
  }, [])

  const toggleSidebar = useCallback(() => {
    setCollapsed((current) => {
      const next = !current
      localStorage.setItem('sidebar-collapsed', next ? '1' : '0')
      return next
    })
  }, [])

  const shell = ['app-shell', collapsed ? 'is-collapsed' : '', locked ? 'is-locked' : ''].filter(Boolean).join(' ')

  return (
    <ShortcutsProvider onToggleSidebar={toggleSidebar} enabled={!phone}>
      <div className={shell}>
        <AppSidebar collapsed={collapsed} onToggle={toggleSidebar} />
        <div className="app-main">
          <main>
            <Suspense fallback={null}>
              <Outlet />
            </Suspense>
          </main>
        </div>
      </div>
    </ShortcutsProvider>
  )
}
