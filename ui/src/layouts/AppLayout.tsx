import { Fragment, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import { Outlet } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'
import PhoneTabs from '../components/PhoneTabs'
import QuickAdd from '../components/QuickAdd'
import { preloadAllWhenIdle } from '../pages'
import { DataVersionContext } from '../hooks/dataVersion'
import { useDueNotifications } from '../hooks/useDueNotifications'
import { useProfile } from '../hooks/profile'
import ShortcutsProvider from '../keys/ShortcutsProvider'

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('sidebar-collapsed') === '1')
  const [dataVersion, setDataVersion] = useState(0)
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

  const dataChanges = useMemo(() => ({ version: dataVersion, changed: () => setDataVersion((current) => current + 1) }), [dataVersion])

  const shell = ['app-shell', collapsed ? 'is-collapsed' : '', locked ? 'is-locked' : '', phone ? 'is-phone' : ''].filter(Boolean).join(' ')

  return (
    <DataVersionContext.Provider value={dataChanges}>
      <ShortcutsProvider onToggleSidebar={toggleSidebar} enabled={!phone}>
        <div className={shell}>
          <AppSidebar collapsed={collapsed} onToggle={toggleSidebar} />
          <div className="app-main">
            <main>
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
      </ShortcutsProvider>
    </DataVersionContext.Provider>
  )
}
