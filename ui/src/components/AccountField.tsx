import type { Account } from '../api/accounts'
import { useT } from '../i18n/context'
import { accountOptions, several } from '../lib/accountsView'
import Dropdown from './Dropdown'

/**
 * The account of a row, in the forms that have one. It is only there with more than one account that is not archived: someone with one
 * account never sees it (docs/accounts-design.md, section 13). An archived account the row is already in shows as "name (archived)" and
 * stays unless another is chosen. With `name`, a form submit carries the value. With `into` (an income: the account the money comes into) it
 * is always there, also with one account, and says "into account". With `mainOnly` (recurring items and debts, which live only in the app's
 * currency) it lists only the accounts in the app's currency, and "more than one account" counts only those; an account in another currency that
 * the row already has still shows, and stays unless another is chosen.
 */
export default function AccountField({
  accounts,
  value,
  onChange,
  name,
  into = false,
  mainOnly = false,
}: {
  /** Every account; null while they are read. */
  accounts: Account[] | null
  value: string
  onChange: (id: string) => void
  name?: string
  into?: boolean
  mainOnly?: boolean
}) {
  const t = useT()
  if (!accounts || !(into || several(accounts, mainOnly))) return null
  return <Dropdown label={t(into ? 'entry.intoAccount' : 'accounts.account')} name={name} value={value} options={accountOptions(accounts, t, [value], mainOnly)} onChange={onChange} />
}
