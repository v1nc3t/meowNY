import type { MessageKey } from '../i18n/en'
import { MAIN_NAME } from './accountsView'

// "Getting started" on the dashboard: the steps from creating the data to pairing a phone, each ticked off by what the app holds.
// It opens for someone who has just created their data, and stays until they hide it. Whether it is open is kept on this device;
// storage can be missing, blocked or full, so every use of it is guarded.

export type StepId =
  | 'vault'
  | 'accounts'
  | 'categories'
  | 'transaction'
  | 'budget'
  | 'recurring'
  | 'stash'
  | 'friend'
  | 'document'
  | 'backup'
  | 'phone'

export type Step = {
  id: StepId
  /** A NavIcon name. */
  icon: string
  title: MessageKey
  text: MessageKey
  to: string
  /** Handed to the page, which opens its form at once. */
  state?: object
  /** 'desktop': not on a phone (backups live on the computer). 'pairing': only the desktop app pairs a phone. */
  only?: 'desktop' | 'pairing'
}

export const STEPS: readonly Step[] = [
  { id: 'vault', icon: 'check', title: 'guide.vault', text: 'guide.vaultText', to: '/app/settings' },
  { id: 'accounts', icon: 'accounts', title: 'guide.accounts', text: 'guide.accountsText', to: '/app/accounts' },
  { id: 'categories', icon: 'categories', title: 'guide.categories', text: 'guide.categoriesText', to: '/app/categories', state: { add: true } },
  { id: 'transaction', icon: 'transactions', title: 'guide.transaction', text: 'guide.transactionText', to: '/app/transactions', state: { add: true } },
  { id: 'budget', icon: 'budgets', title: 'guide.budget', text: 'guide.budgetText', to: '/app/categories' },
  { id: 'recurring', icon: 'recurring', title: 'guide.recurring', text: 'guide.recurringText', to: '/app/recurring' },
  { id: 'stash', icon: 'stash', title: 'guide.stash', text: 'guide.stashText', to: '/app/stashes' },
  { id: 'friend', icon: 'debts', title: 'guide.friend', text: 'guide.friendText', to: '/app/debts', state: { addFriend: true } },
  { id: 'document', icon: 'documents', title: 'guide.document', text: 'guide.documentText', to: '/app/documents' },
  { id: 'backup', icon: 'download', title: 'guide.backup', text: 'guide.backupText', to: '/app/settings?section=backup', only: 'desktop' },
  { id: 'phone', icon: 'sync', title: 'guide.phone', text: 'guide.phoneText', to: '/app/settings?section=phone', only: 'pairing' },
]

/** The steps this device shows: a phone has no backups or pairing of its own, and a served page cannot pair a phone. */
export function stepsFor(where: { phone: boolean; served: boolean }): Step[] {
  return STEPS.filter((step) => {
    if (step.only === 'desktop') return !where.phone
    if (step.only === 'pairing') return !where.phone && !where.served
    return true
  })
}

/**
 * Whether the accounts were set up: there is more than the main account the app starts with, or the main account was renamed or given
 * what it holds.
 */
export function accountsSetUp(accounts: readonly { name: string; openingBalance: string }[]): boolean {
  if (accounts.length !== 1) return accounts.length > 1
  const [main] = accounts
  return main.name !== MAIN_NAME || Number(main.openingBalance) !== 0
}

/** The first step that is not done, or null when all are. */
export function nextStep(steps: readonly Step[], done: ReadonlySet<StepId>): StepId | null {
  return steps.find((step) => !done.has(step.id))?.id ?? null
}

const KEY = 'gettingStarted.v1'

export type GuideState = 'open' | 'hidden'

export function loadGuide(): GuideState | null {
  try {
    const value = localStorage.getItem(KEY)
    return value === 'open' || value === 'hidden' ? value : null
  } catch {
    return null
  }
}

export function saveGuide(state: GuideState): void {
  try {
    localStorage.setItem(KEY, state)
  } catch {
    // Not kept: it shows as it did before next time.
  }
}

/** Data was just created: the steps open on the dashboard. Hands back what it is given, to sit in a promise chain. */
export function startGuide<T>(answer: T): T {
  saveGuide('open')
  return answer
}

/** All data is new (erased or restored): whatever was chosen about the old data goes. */
export function forgetGuide(): void {
  try {
    localStorage.removeItem(KEY)
  } catch {
    // Nothing was stored.
  }
}
