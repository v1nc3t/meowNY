import { createBrowserRouter, RouterProvider, Navigate } from 'react-router-dom'
import PublicLayout from './layouts/PublicLayout'
import AppLayout from './layouts/AppLayout'
import ProtectedRoute from './routes/ProtectedRoute'
import LandingPage from './pages/LandingPage'
import DashboardPage from './pages/DashboardPage'

// Pages

const router = createBrowserRouter([
  // Public Routes
  {
    path: '/',
    element: <PublicLayout />,
    children: [
      { index: true, element: <LandingPage /> },
      // { path: 'login', element: <LoginPage /> },
      // { path: 'signup', element: <SignUpPage /> },
    ],
  },
  // Authenticated App Routes
  {
    path: '/app',
    element: <ProtectedRoute />, // Guards all nested children
    children: [
      {
        element: <AppLayout />, // Renders the Sidebar wrapper
        children: [
          // Automatically redirect /app to /app/dashboard
          { index: true, element: <Navigate to="dashboard" replace /> },
          { path: 'dashboard', element: <DashboardPage /> },
          { path: 'transactions', element: <div>Transactions Mock View</div> },
          { path: 'categories', element: <div>Categories Mock View</div> },
          { path: 'budgets', element: <div>Budgets Mock View</div> },
        ],
      },
    ],
  },
])

export default function App() {
  return <RouterProvider router={router} />
}