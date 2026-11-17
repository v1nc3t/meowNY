import type { KeyboardEvent, ReactNode } from 'react'
import { useT } from '../i18n/context'
import { TAB_LABEL, tabForKey, type EntryTab } from '../lib/entryDialog'
import { tabId } from '../lib/tabs'
import SectionTabs from './SectionTabs'

/**
 * The tabs under the title of an add dialog (expense, income, transfer) and the panel of the chosen one, which is the dialog's `children`. The
 * arrow keys move between the tabs as everywhere (`SectionTabs`); `Alt+1`, `Alt+2` and `Alt+3` choose one from anywhere in the dialog, and the focus
 * goes to that tab, since the field it was in may be gone. With no tabs (a row being changed) there is only the panel.
 */
export default function EntryTabs({
  tabs,
  value,
  label,
  onChange,
  children,
}: {
  tabs: readonly EntryTab[]
  value: EntryTab
  label: string
  onChange: (tab: EntryTab) => void
  children: ReactNode
}) {
  const t = useT()
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const tab = tabForKey(tabs, event)
    if (!tab) return
    event.preventDefault()
    onChange(tab)
    document.getElementById(tabId(tab))?.focus()
  }
  return (
    <div className="entry-dialog" onKeyDown={onKeyDown}>
      {tabs.length > 0 ? <SectionTabs label={label} value={value} onChange={onChange} tabs={tabs.map((id) => ({ id, label: t(TAB_LABEL[id]) }))} /> : null}
      <div className="entry-panel" {...(tabs.length > 0 ? { role: 'tabpanel', 'aria-labelledby': tabId(value) } : {})}>
        {children}
      </div>
    </div>
  )
}
