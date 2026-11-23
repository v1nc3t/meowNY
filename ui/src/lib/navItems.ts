import type { MessageKey } from '../i18n/en'
import type { ShortcutId } from '../keys/shortcuts'
import type { PageName } from '../pages'

/** One page in the navigation: where it is, its name and icon, and the page module to load ahead of time. */
export type NavItem = { to: string; label: MessageKey; icon: string; page: PageName }

const dashboard = { to: '/app/dashboard', label: 'nav.dashboard', icon: 'dashboard', page: 'dashboard' } as const
const transactions = { to: '/app/transactions', label: 'nav.transactions', icon: 'transactions', page: 'transactions' } as const
const accounts = { to: '/app/accounts', label: 'nav.accounts', icon: 'accounts', page: 'accounts' } as const
const stashes = { to: '/app/stashes', label: 'nav.stashes', icon: 'stash', page: 'stashes' } as const
const categories = { to: '/app/categories', label: 'nav.categories', icon: 'categories', page: 'categories' } as const
const recurring = { to: '/app/recurring', label: 'nav.recurring', icon: 'recurring', page: 'recurring' } as const
const debts = { to: '/app/debts', label: 'nav.debts', icon: 'debts', page: 'debts' } as const
const documents = { to: '/app/documents', label: 'nav.documents', icon: 'documents', page: 'documents' } as const
const exportData = { to: '/app/export', label: 'nav.export', icon: 'export', page: 'export' } as const
const sync = { to: '/app/sync', label: 'nav.sync', icon: 'sync', page: 'sync' } as const
const settings = { to: '/app/settings', label: 'nav.settings', icon: 'settings', page: 'settings' } as const

/** The pages of the desktop's sidebar, in its order, each with the shortcut that goes to it. */
export const SIDEBAR_ITEMS = [
  { ...dashboard, shortcut: 'goDashboard' },
  { ...transactions, shortcut: 'goTransactions' },
  { ...accounts, shortcut: 'goAccounts' },
  { ...stashes, shortcut: 'goStashes' },
  { ...categories, shortcut: 'goCategories' },
  { ...recurring, shortcut: 'goRecurring' },
  { ...debts, shortcut: 'goDebts' },
  { ...documents, shortcut: 'goDocuments' },
  { ...exportData, shortcut: 'goExport' },
  { ...settings, shortcut: 'goSettings' },
] as const satisfies readonly (NavItem & { shortcut: ShortcutId })[]

/**
 * The pages of the phone's menu: the sidebar's order, without export (a file cannot be saved from the app on Android yet), and with sync
 * before settings (the phone has no other way to it).
 */
export const PHONE_ITEMS = [dashboard, transactions, accounts, stashes, categories, recurring, debts, documents, sync, settings] as const satisfies readonly NavItem[]
