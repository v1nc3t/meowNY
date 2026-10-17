import { lazy, useEffect, useState, type ReactNode } from 'react'
import { createBrowserRouter, RouterProvider, Navigate } from 'react-router-dom'
import AppLayout from './layouts/AppLayout'
import { pageLoaders } from './pages'
import VaultGate from './vault/VaultGate'
import { syncApi } from './api/sync'
import { useProfile } from './hooks/profile'

const DashboardPage = lazy(pageLoaders.dashboard)
const TransactionsPage = lazy(pageLoaders.transactions)
const CategoriesPage = lazy(pageLoaders.categories)
const BudgetsPage = lazy(pageLoaders.budgets)
const RecurringPage = lazy(pageLoaders.recurring)
const DocumentsPage = lazy(pageLoaders.documents)
const DebtsPage = lazy(pageLoaders.debts)
const FriendPage = lazy(pageLoaders.friend)
const ExportPage = lazy(pageLoaders.export)
const SettingsPage = lazy(pageLoaders.settings)
const SyncPage = lazy(pageLoaders.sync)

/** A page that a phone does not have. */
function NotOnPhone({ children }: { children: ReactNode }) {
  return useProfile() === 'phone' ? <Navigate to="/app/dashboard" replace /> : children
}

/** Opens on the dashboard. A phone that has not met its desktop yet opens on the sync screen: it has nothing to show before that. */
function Home() {
  const phone = useProfile() === 'phone'
  const [target, setTarget] = useState<string | null>(phone ? null : 'dashboard')
  useEffect(() => {
    if (!phone) return
    syncApi.devices().then(
      (paired) => setTarget(paired.length === 0 ? 'sync' : 'dashboard'),
      () => setTarget('dashboard'),
    )
  }, [phone])
  return target ? <Navigate to={target} replace /> : null
}

// No accounts and no public pages: the desktop app opens straight into the app, behind the vault gate.
const router = createBrowserRouter([
  {
    path: '/app',
    element: <AppLayout />,
    children: [
      { index: true, element: <Home /> },
      { path: 'dashboard', element: <DashboardPage /> },
      { path: 'transactions', element: <TransactionsPage /> },
      { path: 'categories', element: <CategoriesPage /> },
      { path: 'budgets', element: <BudgetsPage /> },
      { path: 'recurring', element: <RecurringPage /> },
      { path: 'documents', element: <DocumentsPage /> },
      { path: 'debts', element: <DebtsPage /> },
      { path: 'debts/:personId', element: <FriendPage /> },
      { path: 'export', element: <NotOnPhone><ExportPage /></NotOnPhone> },
      { path: 'settings', element: <SettingsPage /> },
      { path: 'sync', element: <SyncPage /> },
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
