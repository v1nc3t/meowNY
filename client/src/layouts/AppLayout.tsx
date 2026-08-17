import { Outlet } from 'react-router-dom'
import AppSidebar from '../components/AppSidebar'

export default function AppLayout() {
  return (
    <div>
      <AppSidebar />

      {/* Main workspace view */}
      <div>
        <main>
          <Outlet />
        </main>
      </div>
    </div>
  )
}