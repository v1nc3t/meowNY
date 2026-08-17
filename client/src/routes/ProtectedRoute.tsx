import { Navigate, Outlet } from 'react-router-dom'

export default function ProtectedRoute() {
  // Replace this with your actual auth state (e.g., JWT presence or Context state)
  // const isAuthenticated = Boolean(localStorage.getItem('token'))

  // TODO: Replace with real auth check (e.g., localStorage token) when backend is ready
  const MOCK_IS_AUTHENTICATED = true

  if (!MOCK_IS_AUTHENTICATED) {
    return <Navigate to="/login" replace />
  }

  return <Outlet />
}