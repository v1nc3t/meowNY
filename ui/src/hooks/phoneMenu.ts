import { createContext, useContext } from 'react'

/** What a page needs of the phone's menu: to open it (the logo button of `PageHeader`), whether it is open, and the id of its panel (for `aria-controls`). */
export type PhoneMenuState = { open: () => void; isOpen: boolean; id: string }

/** Given by `AppLayout`. Outside of it (a test, a page shown alone) the menu is a closed one that cannot be opened. */
export const PhoneMenuContext = createContext<PhoneMenuState>({ open: () => undefined, isOpen: false, id: 'phone-menu' })

/** The phone's menu: `const menu = usePhoneMenu(); menu.open()`. */
export function usePhoneMenu(): PhoneMenuState {
  return useContext(PhoneMenuContext)
}
