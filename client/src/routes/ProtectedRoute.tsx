import { Navigate, Outlet } from 'react-router-dom'
import { authClient } from '../auth'

export default function ProtectedRoute() {
  const { data: session, isPending } = authClient.useSession()
  if (isPending) return null
  if (!session) return <Navigate to="/login" replace />
  return <Outlet />
}