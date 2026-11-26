import { useEffect, useState } from 'react'
import { categories as categoriesApi, type Category } from '../../api/categories'
import { useAccounts } from '../../hooks/useAccounts'
import { useWidget } from './context'
import type { Filter } from './data'

/** The ids that are still something: an account that exists and is not archived, a category that exists. */
export function liveIds(ids: readonly string[] | undefined, live: ReadonlySet<string>): string[] {
  return (ids ?? []).filter((id) => live.has(id))
}

/** Every category, for the widgets and the settings dialog; null until they are read. */
export function useCategories(): Category[] | null {
  const [list, setList] = useState<Category[] | null>(null)
  useEffect(() => {
    let cancelled = false
    void categoriesApi.list().then(
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
  }, [])
  return list
}

/**
 * The accounts and categories the widget is set to. A choice that points to something deleted or archived is left out, so a widget with no
 * choice left shows everything (docs/dashboard-design.md, section 3). `ready` is false until the accounts and categories are known.
 */
export function useFilter(): { filter: Filter; ready: boolean } {
  const { widget } = useWidget()
  const accounts = useAccounts()
  const categories = useCategories()
  const { accountIds, categoryIds } = widget.settings
  const ready = (!accountIds?.length || accounts !== null) && (!categoryIds?.length || categories !== null)
  const accountLive = liveIds(accountIds, new Set((accounts ?? []).filter((account) => !account.archived).map((account) => account.id)))
  const categoryLive = liveIds(categoryIds, new Set((categories ?? []).map((category) => category.id)))
  return { filter: { accountIds: accountLive.length ? accountLive : undefined, categoryIds: categoryLive.length ? categoryLive : undefined }, ready }
}
