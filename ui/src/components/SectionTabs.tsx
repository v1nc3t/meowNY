import type { KeyboardEvent } from 'react'
import { tabId } from '../lib/tabs'

/**
 * Tabs that switch what a page shows below them (the budgets and the categories on a phone, the recurring items and the due payments).
 * Only one view is shown at a time. The shown tab is the one stop of the tabs for Tab; the arrows, Home and End move to a tab and show it.
 */
export default function SectionTabs<T extends string>({
  tabs,
  value,
  onChange,
  label,
}: {
  tabs: readonly { id: T; label: string }[]
  value: T
  onChange: (id: T) => void
  label: string
}) {
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const at = tabs.findIndex((tab) => tab.id === value)
    const steps: Record<string, number> = { ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: tabs.length - 1 }
    if (event.altKey || event.ctrlKey || event.metaKey || !(event.key in steps)) return
    event.preventDefault()
    const to = (steps[event.key] + tabs.length) % tabs.length
    onChange(tabs[to].id)
    const next = event.currentTarget.children[to] as HTMLElement
    next.focus()
  }

  return (
    <div className="section-tabs ui-tabs wide" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          id={tabId(tab.id)}
          aria-selected={tab.id === value}
          tabIndex={tab.id === value ? 0 : -1}
          onClick={() => onChange(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  )
}
