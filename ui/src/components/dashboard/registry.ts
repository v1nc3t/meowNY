import type { ComponentType } from 'react'
import type { WidgetType } from '../../lib/dashboardWidgets'
import BudgetsWidget from './widgets/BudgetsWidget'
import DebtsWidget from './widgets/DebtsWidget'
import NetWorthWidget from './widgets/NetWorthWidget'
import StashesWidget from './widgets/StashesWidget'
import SummaryWidget from './widgets/SummaryWidget'
import TrendWidget from './widgets/TrendWidget'
import UpcomingWidget from './widgets/UpcomingWidget'

/**
 * The component that draws each widget type. A widget component takes no props: it reads its widget, its settings and the dashboard's
 * context with `useWidget()` (components/dashboard/context.ts), and draws a `WidgetFrame`. The compiler asks for a line here for every type
 * in `SPECS` (lib/dashboardWidgets.ts).
 */
export const WIDGETS: Record<WidgetType, ComponentType> = {
  summary: SummaryWidget,
  trend: TrendWidget,
  networth: NetWorthWidget,
  budgets: BudgetsWidget,
  stashes: StashesWidget,
  upcoming: UpcomingWidget,
  debts: DebtsWidget,
}
