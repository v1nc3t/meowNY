import { useState } from 'react'
import { Outlet } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('sidebar-collapsed') === '1')
  const locked = true

  function toggleSidebar() {
    const next = !collapsed
    localStorage.setItem('sidebar-collapsed', next ? '1' : '0')
    setCollapsed(next)
  }

  const shell = ['app-shell', collapsed ? 'is-collapsed' : '', locked ? 'is-locked' : ''].filter(Boolean).join(' ')

  return (
    <div className={shell}>
      <AppSidebar collapsed={collapsed} onToggle={toggleSidebar} />
      <div className="app-main">
        <main>
          <Outlet />
        </main>
      </div>
    </div>
  )
}
