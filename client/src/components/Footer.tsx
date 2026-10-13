import { Link } from 'react-router-dom'

export default function Footer() {
  return (
    <footer>
      <p>© {new Date().getFullYear()} meowny</p>
      <nav>
        <Link to="/legal/terms">terms</Link>
        <Link to="/legal/privacy">privacy</Link>
      </nav>
    </footer>
  )
}
