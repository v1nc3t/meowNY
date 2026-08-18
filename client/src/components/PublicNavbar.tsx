import { Link } from 'react-router-dom'

export default function PublicNavbar() {
  return (
    <header className="public-navbar">
      <div className="brand">
        <Link to="/">meowNY</Link>
      </div>

      <nav className="nav-links">
        <Link to="/app" className="app-preview-btn">
          App
        </Link>
        <Link to="/login" className="login-btn">
          Log In
        </Link>
        <Link to="/signup" className="signup-btn">
          Sign Up
        </Link>
      </nav>
    </header>
  )
}