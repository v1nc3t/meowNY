/** The `id` of the tab that shows `id`, which the pane below the tabs names in `aria-labelledby`. */
export const tabId = (id: string) => `tab-${id}`

/** The props of the pane that tab `id` shows. The pane is a stop of its own, so the keyboard can scroll it, and the tab names it. */
export function tabPanelProps(id: string) {
  return { role: 'tabpanel', tabIndex: 0, 'aria-labelledby': tabId(id) } as const
}
