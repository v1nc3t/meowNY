import type { ComponentType } from 'react'
import type { WidgetType } from '../../lib/dashboardWidgets'
import AccountsWidget from './widgets/AccountsWidget'
import AccountWidget from './widgets/AccountWidget'
import BudgetsWidget from './widgets/BudgetsWidget'
import CashFlowWidget from './widgets/CashFlowWidget'
import CategoryWidget from './widgets/CategoryWidget'
import DebtsWidget from './widgets/DebtsWidget'
import LoansWidget from './widgets/LoansWidget'
import NetWorthTimeWidget from './widgets/NetWorthTimeWidget'
import NetWorthWidget from './widgets/NetWorthWidget'
import RecentWidget from './widgets/RecentWidget'
import SavingsRateWidget from './widgets/SavingsRateWidget'
import SpendingWidget from './widgets/SpendingWidget'
import StashesWidget from './widgets/StashesWidget'
import StashWidget from './widgets/StashWidget'
import SummaryWidget from './widgets/SummaryWidget'
import TopExpensesWidget from './widgets/TopExpensesWidget'
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
  cashflow: CashFlowWidget,
  networth: NetWorthWidget,
  networthtime: NetWorthTimeWidget,
  account: AccountWidget,
  accounts: AccountsWidget,
  budgets: BudgetsWidget,
  category: CategoryWidget,
  spending: SpendingWidget,
  topexpenses: TopExpensesWidget,
  recent: RecentWidget,
  upcoming: UpcomingWidget,
  stashes: StashesWidget,
  stash: StashWidget,
  debts: DebtsWidget,
  loans: LoansWidget,
  savingsrate: SavingsRateWidget,
}
