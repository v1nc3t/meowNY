import type { Account } from '../api/accounts'
import { useT } from '../i18n/context'
import { accountOptions, several } from '../lib/accountsView'
import Dropdown from './Dropdown'

/**
 * The account of a row, in the forms that have one. It is only there with more than one account that is not archived: someone with one
 * account never sees it (docs/accounts-design.md, section 13). An archived account the row is already in shows as "name (archived)" and
 * stays unless another is chosen. With `name`, a form submit carries the value.
 */
export default function AccountField({
  accounts,
  value,
  onChange,
  name,
}: {
  /** Every account; null while they are read. */
  accounts: Account[] | null
  value: string
  onChange: (id: string) => void
  name?: string
}) {
  const t = useT()
  if (!accounts || !several(accounts)) return null
  return <Dropdown label={t('accounts.account')} name={name} value={value} options={accountOptions(accounts, t, [value])} onChange={onChange} />
}
