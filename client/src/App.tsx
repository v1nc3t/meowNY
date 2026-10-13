import { createBrowserRouter, RouterProvider, Navigate } from 'react-router-dom'
import PublicLayout from './layouts/PublicLayout'
import AppLayout from './layouts/AppLayout'
import ProtectedRoute from './routes/ProtectedRoute'
import LandingPage from './pages/LandingPage'
import LoginPage from './pages/LoginPage'
import SignupPage from './pages/SignupPage'
import ForgotPasswordPage from './pages/ForgotPasswordPage'
import ResetPasswordPage from './pages/ResetPasswordPage'
import OnboardingPage from './pages/OnboardingPage'
import TermsPage from './pages/TermsPage'
import PrivacyPage from './pages/PrivacyPage'
import DashboardPage from './pages/DashboardPage'
import TransactionsPage from './pages/TransactionsPage'
import CategoriesPage from './pages/CategoriesPage'
import BudgetsPage from './pages/BudgetsPage'
import AccountPage from './pages/AccountPage'
import SettingsPage from './pages/SettingsPage'
import VaultGate from './vault/VaultGate'

// Pages

const router = createBrowserRouter([
  // Public Routes
  {
    path: '/',
    element: <PublicLayout />,
    children: [
      { index: true, element: <LandingPage /> },
      { path: 'login', element: <LoginPage /> },
      { path: 'signup', element: <SignupPage /> },
      { path: 'forgot-password', element: <ForgotPasswordPage /> },
      { path: 'reset-password', element: <ResetPasswordPage /> },
      { path: 'legal/terms', element: <TermsPage /> },
      { path: 'legal/privacy', element: <PrivacyPage /> },
      {
        path: 'onboarding',
        element: <ProtectedRoute />,
        children: [{ index: true, element: <OnboardingPage /> }],
      },
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
          { path: 'transactions', element: <TransactionsPage /> },
          { path: 'categories', element: <CategoriesPage /> },
          { path: 'budgets', element: <BudgetsPage /> },
          { path: 'account', element: <AccountPage /> },
          { path: 'settings', element: <SettingsPage /> },
        ],
      },
    ],
  },
])

export default function App() {
  return (
    <VaultGate>
      <RouterProvider router={router} />
    </VaultGate>
  )
}