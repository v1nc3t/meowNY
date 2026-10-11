import { Outlet } from 'react-router-dom'
import PublicNavbar from '../components/PublicNavbar'
import Footer from '../components/Footer'

export default function PublicLayout() {
  return (
    <div className="public-shell">
      <PublicNavbar />
      
      {/* Dynamic page content (HomePage, AboutPage, etc.) renders here */}
      <main>
        <Outlet />
      </main>

      <Footer />
    </div>
  )
}