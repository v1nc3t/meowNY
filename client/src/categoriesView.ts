export type Group = { id: string; name: string }
export type Category = { id: string; name: string; type: string; categoryGroupId: string | null }
export type Kind = 'ALL' | 'INCOME' | 'EXPENSE'

export function visibleGroups(
  groups: Group[],
  categories: Category[],
  kind: Kind,
  query: string,
): { group: Group; categories: Category[] }[] {
  const q = query.trim().toLowerCase()
  return groups.flatMap((group) => {
    const members = categories.filter(
      (category) => category.categoryGroupId === group.id && (kind === 'ALL' || category.type === kind),
    )
    const nameHit = q.length > 0 && group.name.toLowerCase().includes(q)
    const shown =
      q.length === 0 || nameHit ? members : members.filter((category) => category.name.toLowerCase().includes(q))
    const include = q.length === 0 ? kind === 'ALL' || members.length > 0 : nameHit || shown.length > 0
    return include ? [{ group, categories: shown }] : []
  })
}

export function visibleUngrouped(groups: Group[], categories: Category[], kind: Kind, query: string): Category[] {
  const q = query.trim().toLowerCase()
  const ids = new Set(groups.map((group) => group.id))
  return categories.filter((category) => {
    if (category.categoryGroupId !== null && ids.has(category.categoryGroupId)) return false
    if (kind !== 'ALL' && category.type !== kind) return false
    return q.length === 0 || category.name.toLowerCase().includes(q)
  })
}
