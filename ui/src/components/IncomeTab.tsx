import { shiftMonth } from '../api/months'
import { useBudgets } from '../hooks/useBudgets'
import type { MoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useSticky } from '../lib/sticky'
import { tabPanelProps } from '../lib/tabs'
import BudgetHalf from './BudgetHalf'
import MonthNav from './MonthNav'
import Button from './ui/Button'

/**
 * The income tab of the accounts page: the income side of the budgets, as the categories page had it (docs/accounts-design.md, section
 * 20.1). The month is the one the categories page remembers, so both pages look at the same month.
 */
export default function IncomeTab({ settings, thisMonth }: { settings: MoneySettings; thisMonth: string }) {
  const t = useT()
  const [month, setMonth] = useSticky('categories.month', () => thisMonth)
  const budgets = useBudgets({ type: 'INCOME', month, thisMonth, settings })

  // Keyboard: [ and ] step through months, t jumps to this month, n adds a category.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(thisMonth))
  useShortcut('new', budgets.add)

  return (
    <div className="tx-pane wide budget-view" {...tabPanelProps('income')}>
      <div className="tx-actions acct-income-bar">
        <Button onClick={budgets.add} {...tip('new', t('categories.add'))}>
          {t('categories.add')}
        </Button>
        <MonthNav month={month} thisMonth={thisMonth} onChange={setMonth} />
      </div>
      <BudgetHalf {...budgets.half} hideTitle />
      {budgets.dialogs}
    </div>
  )
}
