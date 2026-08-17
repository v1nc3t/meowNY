import { NavLink, Link } from 'react-router-dom'

export default function AppSidebar() {
  return (
    <aside className="app-sidebar">
      <div className="sidebar-header">
        <Link to="/app/dashboard" className="app-logo">
          meowNY
        </Link>
      </div>

      <nav className="sidebar-nav">
        <NavLink 
          to="/app/dashboard" 
          className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}
        >
          Dashboard
        </NavLink>

        <NavLink 
          to="/app/transactions" 
          className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}
        >
          Transactions
        </NavLink>

        <NavLink 
          to="/app/categories" 
          className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}
        >
          Categories & Groups
        </NavLink>

        <NavLink 
          to="/app/budgets" 
          className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}
        >
          Budgets
        </NavLink>
      </nav>

      <div className="sidebar-footer">
        <Link to="/" className="exit-btn">
          Exit to Website
        </Link>
      </div>
    </aside>
  )
}