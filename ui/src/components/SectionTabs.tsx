import { tabId } from '../lib/tabs'
import { Tabs, TabsList, TabsTrigger } from './ui/tabs'

/**
 * Tabs that switch what a page shows below them (the budgets and the categories on a phone, the recurring items and the due payments), on the
 * shadcn Tabs. Only one view is shown at a time. The shown tab is the one stop of the tabs for Tab; the arrows, Home and End move to a tab and
 * show it. The pane under the tabs is the page's own (`tabPanelProps`), so the tabs are one element, the list.
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
  return (
    <Tabs asChild value={value} onValueChange={(id) => onChange(id as T)}>
      <TabsList className="section-tabs wide" aria-label={label}>
        {tabs.map((tab) => (
          <TabsTrigger key={tab.id} value={tab.id} id={tabId(tab.id)} aria-controls={undefined}>
            {tab.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  )
}
