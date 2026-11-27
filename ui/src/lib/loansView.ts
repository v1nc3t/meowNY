import { isLiability, type Account, type Payoff } from '../api/accounts'
import type { FactItem } from '../components/Facts'
import type { MessageKey } from '../i18n/en'
import { fromMinor, toMinor } from './money'
import { monthWords } from './stashView'

// What the screens show of the accounts you owe on (docs/loans-deposits-design.md, section 10). Nothing here works out interest or a payoff: the app does.

type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string

/** Every credit, loan and mortgage in the person's order, the ones that are not archived first. */
export function liabilities(accounts: readonly Account[]): Account[] {
  const all = accounts.filter((account) => isLiability(account.kind))
  return [...all.filter((account) => !account.archived), ...all.filter((account) => account.archived)]
}

/**
 * What is owed on all of them, in the app's currency: each liability's balance in the app's currency (an account in another currency at its rate)
 * turned positive, and one that was overpaid counting as 0. Never below 0.
 */
export function totalOwed(accounts: readonly Account[]): string {
  let total = 0n
  for (const account of accounts) {
    if (!isLiability(account.kind)) continue
    const balance = toMinor(account.balanceInMain) ?? 0n
    if (balance < 0n) total -= balance
  }
  return fromMinor(total)
}

/** "3.9 % per year", or '' for an account with no rate. */
export function rateWords(account: Pick<Account, 'interestRate' | 'interestPeriod'>, t: Translate): string {
  return account.interestRate === null || account.interestPeriod === null ? '' : t('accounts.rateShown', { rate: account.interestRate, period: t(`accounts.period.${account.interestPeriod}`) })
}

/** When the debt is paid off at its installment: "june 2051", "never", or '' when there is no installment or nothing is owed. */
export function payoffWords(payoff: Payoff | null | undefined, locale: string, t: Translate): string {
  if (!payoff) return ''
  return 'never' in payoff ? t('loans.never') : monthWords(`${payoff.month}-01`, locale)
}

/**
 * What a credit, a loan or a mortgage owes, costs and when it ends, as the pairs of `Facts`: what is owed, the rate, the installment, a month of
 * interest and the payoff (a month, or "never" when the installment does not beat the interest), and the state once nothing is owed any more.
 * A pair with nothing to say is left out by `Facts`. The numbers are the app's (`Account.liability`); `money` writes an amount in the account's
 * currency.
 */
export function loanFacts(account: Account, money: (amount: string) => string, locale: string, t: Translate): FactItem[] {
  const liability = account.liability
  return [
    { label: t('accounts.owed'), value: liability ? money(liability.owed) : '' },
    { label: t('accounts.rate'), value: rateWords(account, t) },
    { label: t('accounts.installment'), value: account.installment === null ? '' : money(account.installment) },
    { label: t('loans.interestPerMonth'), value: liability?.interestPerMonth == null ? '' : money(liability.interestPerMonth) },
    { label: t('loans.payoff'), value: payoffWords(liability?.payoff, locale, t) },
    { label: t('debts.colStatus'), value: liability && liability.state !== 'OWING' ? t(`loans.state.${liability.state}`) : '' },
  ]
}

/** The two tabs of the debts page: what friends owe, and the accounts you owe on. */
export type DebtsTab = 'friends' | 'debts'

/** The tab an address names: `?tab=debts` is the debts, no tab and a word nobody knows are the friends (the page as it was). */
export function debtsTabOf(search: string | URLSearchParams): DebtsTab {
  return new URLSearchParams(search).get('tab') === 'debts' ? 'debts' : 'friends'
}

/** Where a tab is in the address: the page's own for the friends, `?tab=debts` for the debts. */
export function debtsTabQuery(tab: DebtsTab): Record<string, string> {
  return tab === 'debts' ? { tab: 'debts' } : {}
}
