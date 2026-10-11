import { Link } from 'react-router-dom'

export default function PublicNavbar() {
  return (
    <header className="public-navbar">
      <div className="brand">
        <Link to="/">meowny</Link>
      </div>

      <nav className="nav-links">
        <Link to="/app" className="app-preview-btn">
          app
        </Link>
        <Link to="/login" className="login-btn">
          log in
        </Link>
        <Link to="/signup" className="signup-btn">
          sign up
        </Link>
      </nav>
    </header>
  )
}
