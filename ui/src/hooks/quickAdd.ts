import { createContext, useContext } from 'react'

/** The main action of the page that is shown (add account, new stash…), offered first by the round add button of a phone. */
export type PageAdd = { label: string; onClick: () => void }

/**
 * Given by `AppLayout`. `fab` is true where a phone shows the round add button: there the page's main action is one of its lines, not a
 * second "+" in the page's top line. `set` is how `PageHeader` hands its main action over (null when the page goes).
 */
export const PageAddContext = createContext<{ fab: boolean; set: (add: PageAdd | null) => void }>({ fab: false, set: () => undefined })

export function usePageAdd() {
  return useContext(PageAddContext)
}
