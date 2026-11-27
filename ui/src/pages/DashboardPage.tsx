import { useState, type KeyboardEvent } from 'react'
import { shiftMonth } from '../api/months'
import { useToday, type Today } from '../api/today'
import ConfirmModal from '../components/ConfirmModal'
import CatalogDialog from '../components/dashboard/CatalogDialog'
import DashboardGrid from '../components/dashboard/DashboardGrid'
import EditBar from '../components/dashboard/EditBar'
import GettingStarted from '../components/dashboard/GettingStarted'
import SettingsDialog from '../components/dashboard/SettingsDialog'
import PageHeader from '../components/PageHeader'
import YearNav from '../components/YearNav'
import { newWidgetId, useDashboardLayout } from '../hooks/useDashboardLayout'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useProfile } from '../hooks/profile'
import { useI18n } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { addWidget, setWidgetSettings } from '../lib/dashboardLayout'
import { SPECS } from '../lib/dashboardWidgets'
import { loadGuide, saveGuide } from '../lib/gettingStarted'
import { useSticky } from '../lib/sticky'

export default function DashboardPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <DashboardView settings={settings} now={now} />
}

/**
 * The dashboard: a grid of widgets the person arranges (docs/dashboard-design.md). The period control in the header steps the month that "this
 * month" and "this year" follow; "customize dashboard" in the "more" menu turns the widgets into things to drag, size, set and remove.
 */
function DashboardView({ settings, now }: { settings: MoneySettings; now: Today }) {
  const { t } = useI18n()
  const profile = useProfile()
  const [month, setMonth] = useSticky('dashboard.month', () => now.month)
  const { layout, update, reset } = useDashboardLayout(profile)
  const [editing, setEditing] = useState(false)
  const [catalog, setCatalog] = useState(false)
  const [asking, setAsking] = useState(false)
  const [settingsOf, setSettingsOf] = useState<string | null>(null)
  // "Getting started" opens after the data is created and stays until it is hidden; the "more" menu brings it back.
  const [guide, setGuide] = useState(loadGuide)

  function showGuide(open: boolean) {
    const state = open ? 'open' : 'hidden'
    saveGuide(state)
    setGuide(state)
  }

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(now.month))

  // Escape leaves edit mode, unless it was for a dialog, a menu or a pop-up opened from a widget.
  function onKeyDown(event: KeyboardEvent) {
    if (!editing || event.key !== 'Escape' || event.defaultPrevented) return
    if (event.target instanceof Element && event.target.closest('[role="dialog"], [role="menu"], [data-slot="popover-content"], dialog')) return
    setEditing(false)
  }

  const widget = settingsOf ? layout.widgets.find((each) => each.id === settingsOf) : undefined

  return (
    <section className="tx-page" onKeyDown={onKeyDown}>
      <PageHeader
        title={t('dashboard.title')}
        more={[
          { label: t('dashboard.customize'), icon: 'pencil', onSelect: () => setEditing(true) },
          { label: t('dashboard.resetLayout'), icon: 'recurring', onSelect: () => setAsking(true) },
          ...(guide === 'open' ? [] : [{ label: t('guide.show'), icon: 'help', onSelect: () => showGuide(true) }]),
        ]}
      >
        <YearNav month={month} thisMonth={now.month} onChange={setMonth} />
      </PageHeader>
      <div className="tx-pane wide dash">
        {editing ? <EditBar onAdd={() => setCatalog(true)} onReset={() => setAsking(true)} onDone={() => setEditing(false)} /> : null}
        {guide === 'open' && !editing ? <GettingStarted onHide={() => showGuide(false)} /> : null}
        <DashboardGrid
          layout={layout}
          profile={profile}
          editing={editing}
          context={{ money: settings, today: now, month, setMonth, phone: profile === 'phone' }}
          onChange={update}
          onSettings={setSettingsOf}
        />
      </div>
      {catalog ? (
        <CatalogDialog
          profile={profile}
          onClose={() => setCatalog(false)}
          onChoose={(type) => {
            update((current) => addWidget(current, type, profile, newWidgetId(current)))
            setCatalog(false)
          }}
        />
      ) : null}
      {widget ? (
        <SettingsDialog
          widget={widget}
          name={widget.settings.title ?? t(SPECS[widget.type].name)}
          onClose={() => setSettingsOf(null)}
          onSave={(next) => {
            update((current) => setWidgetSettings(current, widget.id, next))
            setSettingsOf(null)
          }}
        />
      ) : null}
      {asking ? (
        <ConfirmModal
          title={t('dashboard.resetTitle')}
          text={t('dashboard.resetText')}
          onClose={() => setAsking(false)}
          onConfirm={() => {
            reset()
            setAsking(false)
          }}
        />
      ) : null}
    </section>
  )
}
