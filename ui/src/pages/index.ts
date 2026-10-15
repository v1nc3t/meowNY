/**
 * Every page loads on its own, so the app opens with only what the first screen needs.
 * The loaders live here so the router can lazy-load them and the sidebar can start loading
 * a page before it is opened (on hover or focus), which makes opening it instant.
 */
export const pageLoaders = {
  dashboard: () => import('./DashboardPage'),
  transactions: () => import('./TransactionsPage'),
  categories: () => import('./CategoriesPage'),
  budgets: () => import('./BudgetsPage'),
  recurring: () => import('./RecurringPage'),
  export: () => import('./ExportPage'),
  settings: () => import('./SettingsPage'),
} as const

export type PageName = keyof typeof pageLoaders

/** Starts loading a page. Safe to call many times: the module is fetched once. */
export function preloadPage(name: PageName): void {
  void pageLoaders[name]()
}

/** Loads every page once the app is idle, so later navigation never waits. */
export function preloadAllWhenIdle(): void {
  const run = () => (Object.keys(pageLoaders) as PageName[]).forEach(preloadPage)
  if (typeof requestIdleCallback === 'function') requestIdleCallback(run, { timeout: 3000 })
  else setTimeout(run, 1500)
}
