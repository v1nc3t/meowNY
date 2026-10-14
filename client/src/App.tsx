import { lazy } from 'react'
import { createBrowserRouter, RouterProvider, Navigate } from 'react-router-dom'
import AppLayout from './layouts/AppLayout'
import { pageLoaders } from './pages'
import VaultGate from './vault/VaultGate'

const DashboardPage = lazy(pageLoaders.dashboard)
const TransactionsPage = lazy(pageLoaders.transactions)
const CategoriesPage = lazy(pageLoaders.categories)
const BudgetsPage = lazy(pageLoaders.budgets)
const RecurringPage = lazy(pageLoaders.recurring)
const SettingsPage = lazy(pageLoaders.settings)

// No accounts and no public pages: the desktop app opens straight into the app, behind the vault gate.
const router = createBrowserRouter([
  {
    path: '/app',
    element: <AppLayout />,
    children: [
      { index: true, element: <Navigate to="dashboard" replace /> },
      { path: 'dashboard', element: <DashboardPage /> },
      { path: 'transactions', element: <TransactionsPage /> },
      { path: 'categories', element: <CategoriesPage /> },
      { path: 'budgets', element: <BudgetsPage /> },
      { path: 'recurring', element: <RecurringPage /> },
      { path: 'settings', element: <SettingsPage /> },
    ],
  },
  { path: '*', element: <Navigate to="/app" replace /> },
])

export default function App() {
  return (
    <VaultGate>
      <RouterProvider router={router} />
    </VaultGate>
  )
}
