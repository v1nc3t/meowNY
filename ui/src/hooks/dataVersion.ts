import { createContext, useContext } from 'react'

/**
 * A number that goes up when something outside the open page changed the data (the quick add of a phone). The layout draws
 * the page again from the start when it changes, so the page shows what is there now.
 */
export const DataVersionContext = createContext<{ version: number; changed: () => void }>({ version: 0, changed: () => undefined })

export function useDataChanged(): () => void {
  return useContext(DataVersionContext).changed
}
