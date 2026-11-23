import { useEffect, useRef, useState } from 'react'
import { budgets as budgetsApi, type MonthHistory } from '../api/budgets'
import { monthLabel, shiftMonth } from '../api/months'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'
import PageHeader from '../components/PageHeader'
import BudgetHalf from '../components/BudgetHalf'
import MonthNav from '../components/MonthNav'
import SectionTabs from '../components/SectionTabs'
import Button from '../components/ui/Button'
import { CardTitle } from '../components/ui/Card'
import { useToday } from '../api/today'
import { useMoneySettings, type MoneySettings } from '../hooks/useMoneySettings'
import { useBudgets } from '../hooks/useBudgets'
import { useProfile } from '../hooks/profile'
import { useSwipeHandler } from '../hooks/swipe'
import { categoryTabs, earnedClass, shownTab, spentClass, tabOfSearch, tabQuery, type CategoryTab } from '../lib/categoriesView'
import { step } from '../lib/swipe'
import { useSticky } from '../lib/sticky'
import { tabPanelProps } from '../lib/tabs'
import { useI18n } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { useRowNavigation } from '../keys/useRowNavigation'

const SIDES = ['EXPENSE', 'INCOME'] as const

export default function CategoriesPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <CategoriesView settings={settings} thisMonth={now.month} />
}

/** The budgets of the expenses and of the income side by side, and the history of both sides under them; on a phone each is a tab. */
function CategoriesView({ settings, thisMonth }: { settings: MoneySettings; thisMonth: string }) {
  const { t, language } = useI18n()
  const phone = useProfile() === 'phone'
  const [month, setMonth] = useSticky('categories.month', () => thisMonth)
  const [history, setHistory] = useState<MonthHistory[] | null>(null)
  const paneRef = useRef<HTMLDivElement>(null)
  // the tab is in the address (`?tab=income`), so other pages can link to the income, and a reload and back keep it
  const [params, setParams] = useSearchParams()
  const chosenTab = tabOfSearch(params)
  const budgets = useBudgets({ month, thisMonth, settings })
  const { money } = budgets

  useEffect(() => {
    let cancelled = false
    void budgetsApi.history().then(
      (rows) => {
        if (!cancelled) setHistory(rows)
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [budgets.reload])

  // On a phone the parts of the page are tabs (the history only once there is some), and a swipe moves between them before it moves to another page.
  const tabs = categoryTabs(history !== null && history.length > 0)
  const tab = shownTab(tabs, chosenTab)
  const setTab = (next: CategoryTab) => setParams(tabQuery(next), { replace: true })
  useSwipeHandler((direction) => {
    const next = step(tabs, tab, direction)
    if (next === null) return false
    setTab(next)
    return true
  }, phone)
  // A new category starts as the side that is shown: the tab on a phone, the expenses on the computer (where each side has its own add as well).
  const addSide = phone && tab === 'INCOME' ? 'INCOME' : 'EXPENSE'

  // On the computer every part is on the page: an address that names the income or the history scrolls to it, once the page has its height.
  const ready = history !== null && budgets.ready
  useEffect(() => {
    if (phone || !ready || chosenTab === 'EXPENSE') return
    paneRef.current?.querySelector(chosenTab === 'INCOME' ? '[data-side="INCOME"]' : '.budget-history')?.scrollIntoView()
  }, [phone, ready, chosenTab])

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(thisMonth))
  // n adds a category, j and k move between rows.
  useShortcut('new', () => budgets.add(addSide))

  // The phone's round button sends the person here to add a category: the form opens at once, once.
  const location = useLocation()
  const navigate = useNavigate()
  useEffect(() => {
    if ((location.state as { add?: boolean } | null)?.add !== true) return
    budgets.add(addSide)
    navigate({ pathname: location.pathname, search: location.search }, { replace: true, state: null })
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])
  useRowNavigation()

  // The two sides, one beside the other on the computer, one at a time on a phone.
  const halves = SIDES.map((side) => <BudgetHalf key={side} {...budgets.half(side)} hideTitle={phone} />)

  // The months, each a row you click to see that month above; spent is green within its budget and red above it, earned green above its budget.
  const historyList = history ? (
    <ul className="card-list hist-list">
      <li className="list-head" aria-hidden="true">
        <span>{t('budgets.colMonth')}</span>
        <span className="is-end">{t('budgets.colSpent')}</span>
        <span className="is-end">{t('budgets.colExpenseBudget')}</span>
        <span className="is-end">{t('budgets.colEarned')}</span>
        <span className="is-end">{t('budgets.colIncomeBudget')}</span>
      </li>
      {history.map((row) => {
        return (
          <li key={row.month} className={row.month === month ? 'card-item is-selected' : 'card-item'}>
            <span className="cell-name">
              <Button
                variant="link"
                className="card-name"
                aria-current={row.month === month ? 'date' : undefined}
                onClick={() => {
                  setMonth(row.month)
                  paneRef.current?.scrollTo({ top: 0 })
                }}
              >
                {monthLabel(row.month, language)}
              </Button>
            </span>
            <span className="sr-only hist-label">{t('budgets.colSpent')}</span>
            <span className={spentClass(row.expenseActual, row.expenseBudget)}>{money(row.expenseActual)}</span>
            <span className="cell-sub-end hist-budget">
              <span className="sr-only">{t('budgets.colExpenseBudget')}</span>
              {row.expenseBudget === null ? '-' : money(row.expenseBudget)}
            </span>
            <span className="sr-only hist-label">{t('budgets.colEarned')}</span>
            <span className={earnedClass(row.incomeActual, row.incomeBudget)}>{money(row.incomeActual)}</span>
            <span className="cell-sub-end hist-budget">
              <span className="sr-only">{t('budgets.colIncomeBudget')}</span>
              {row.incomeBudget === null ? '-' : money(row.incomeBudget)}
            </span>
          </li>
        )
      })}
    </ul>
  ) : null

  return (
    <section className="tx-page budget-page">
      <PageHeader
        title={t('categories.title')}
        // The phone has the round add button for this.
        actions={[!phone && { label: t('categories.add'), icon: 'add', primary: true, shortcut: 'new', onClick: () => budgets.add(addSide) }]}
      >
        <MonthNav month={month} thisMonth={thisMonth} onChange={setMonth} />
      </PageHeader>
      {budgets.dialogs}
      {phone ? (
        <>
          <SectionTabs
            label={t('categories.title')}
            value={tab}
            onChange={setTab}
            tabs={tabs.map((name) => ({ id: name, label: t(name === 'EXPENSE' ? 'budgets.expenses' : name === 'INCOME' ? 'budgets.income' : 'budgets.history') }))}
          />
          <div className="tx-pane wide budget-view" {...tabPanelProps(tab)} ref={paneRef}>
            {tab === 'EXPENSE' ? halves[0] : null}
            {tab === 'INCOME' ? halves[1] : null}
            {tab === 'HISTORY' ? historyList : null}
          </div>
        </>
      ) : (
        <div className="tx-pane wide budget-view" ref={paneRef}>
          <div className="budget-split">{halves}</div>
          {history && history.length > 0 ? (
            <section className="budget-history">
              <CardTitle>{t('budgets.history')}</CardTitle>
              {historyList}
            </section>
          ) : null}
        </div>
      )}
    </section>
  )
}
