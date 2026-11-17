import { useEffect, useState } from 'react'
import { accounts as accountsApi, type Account } from '../api/accounts'

/**
 * Every account, archived ones too, for the forms and lists that name one; null until they are read. Read again when `reload` changes.
 * A failure reads as no accounts: the forms then have no account field and rows go to the main account, as with one account.
 */
export function useAccounts(reload = 0): Account[] | null {
  const [list, setList] = useState<Account[] | null>(null)
  useEffect(() => {
    let cancelled = false
    void accountsApi.list().then(
      (loaded) => {
        if (!cancelled) setList(loaded)
      },
      () => {
        if (!cancelled) setList((current) => current ?? [])
      },
    )
    return () => {
      cancelled = true
    }
  }, [reload])
  return list
}
