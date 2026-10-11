import { useState } from 'react'
import { Outlet } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('sidebar-collapsed') === '1')

  function toggleSidebar() {
    const next = !collapsed
    localStorage.setItem('sidebar-collapsed', next ? '1' : '0')
    setCollapsed(next)
  }

  return (
    <div className={collapsed ? 'app-shell is-collapsed' : 'app-shell'}>
      <AppSidebar collapsed={collapsed} onToggle={toggleSidebar} />
      <div className="app-main">
        <main>
          <Outlet />
        </main>
      </div>
    </div>
  )
}
