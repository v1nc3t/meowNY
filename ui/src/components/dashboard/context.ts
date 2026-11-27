import { createContext, useContext } from 'react'
import type { Today } from '../../api/today'
import type { MoneySettings } from '../../hooks/useMoneySettings'
import type { Widget } from '../../lib/dashboardLayout'
import { resolvePeriod, type Range } from '../../lib/dashboardPeriod'
import { SPECS } from '../../lib/dashboardWidgets'

/** What a widget knows about the dashboard it is on. */
export type DashboardContext = {
  money: MoneySettings
  today: Today
  /** The month the dashboard shows ("YYYY-MM"): what "this month" and "this year" follow. */
  month: string
  setMonth: (month: string) => void
  phone: boolean
}

export type WidgetContextValue = DashboardContext & { widget: Widget }

export const WidgetContext = createContext<WidgetContextValue | null>(null)

/** The widget being drawn, with the dashboard's context. Every component of components/dashboard/widgets/ reads it. */
export function useWidget(): WidgetContextValue {
  const value = useContext(WidgetContext)
  if (!value) throw new Error('useWidget outside a widget')
  return value
}

/** The days of the widget's period (its own, or the one its kind starts with), counted from the dashboard's month. */
export function usePeriodRange(): Range {
  const { widget, month } = useWidget()
  return resolvePeriod(widget.settings.period, SPECS[widget.type].settings.period ?? 'thisMonth', month)
}
