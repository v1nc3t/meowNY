import { useEffect, useRef, useState } from 'react'
import { budgets as budgetsApi, type MonthHistory } from '../api/budgets'
import { monthLabel, shiftMonth } from '../api/months'
import { useLocation, useNavigate } from 'react-router-dom'
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
import { categoryTabs, earnedClass, shownTab, spentClass } from '../lib/categoriesView'
import { step } from '../lib/swipe'
import { useSticky } from '../lib/sticky'
import { tabPanelProps } from '../lib/tabs'
import { useI18n } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

export default function CategoriesPage() {
  const { settings, error } = useMoneySettings()
  const now = useToday()
  if (error) return <p className="notice">{error}</p>
  if (!settings || !now) return null
  return <CategoriesView settings={settings} thisMonth={now.month} />
}

/** The expense budgets and the history of both sides. The income budgets are on the accounts page (docs/accounts-design.md, section 20.1). */
function CategoriesView({ settings, thisMonth }: { settings: MoneySettings; thisMonth: string }) {
  const { t, language } = useI18n()
  const phone = useProfile() === 'phone'
  const [month, setMonth] = useSticky('categories.month', () => thisMonth)
  const [history, setHistory] = useState<MonthHistory[] | null>(null)
  const paneRef = useRef<HTMLDivElement>(null)
  const budgets = useBudgets({ type: 'EXPENSE', month, thisMonth, settings })
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

  // Keyboard: [ and ] step through months, t jumps to this month.
  useShortcut('prev', () => setMonth((current) => shiftMonth(current, -1)))
  useShortcut('next', () => setMonth((current) => shiftMonth(current, 1)))
  useShortcut('today', () => setMonth(thisMonth))
  // n adds a category, j and k move between rows.
  useShortcut('new', budgets.add)

  // The phone's round button sends the person here to add a category: the form opens at once, once.
  const location = useLocation()
  const navigate = useNavigate()
  useEffect(() => {
    if ((location.state as { add?: boolean } | null)?.add !== true) return
    budgets.add()
    navigate(location.pathname, { replace: true, state: null })
    // each time the page is asked, when it opens and when it is open already; the request is taken out of the location, so once
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state])
  useRowNavigation()

  // On a phone the parts of the page are tabs (the history only once there is some), and a swipe moves between them before it moves to another page.
  const tabs = categoryTabs(history !== null && history.length > 0)
  const [chosenTab, setTab] = useState<string>('EXPENSE')
  const tab = shownTab(tabs, chosenTab)
  useSwipeHandler((direction) => {
    const next = step(tabs, tab, direction)
    if (next === null) return false
    setTab(next)
    return true
  }, phone)

  const half = <BudgetHalf {...budgets.half} hideTitle={phone} />

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
    <section className="tx-page">
      <div className="tx-title budget-title one-line">
        <h1>{t('categories.title')}</h1>
        <div className="tx-actions">
          {/* The phone has the round add button for this. */}
          {phone ? null : (
            <Button onClick={budgets.add} {...tip('new', t('categories.add'))}>
              {t('categories.add')}
            </Button>
          )}
          <MonthNav month={month} thisMonth={thisMonth} onChange={setMonth} />
        </div>
      </div>
      {budgets.dialogs}
      {phone ? (
        <>
          {tabs.length > 1 ? (
            <SectionTabs
              label={t('categories.title')}
              value={tab}
              onChange={setTab}
              tabs={tabs.map((name) => ({ id: name, label: t(name === 'EXPENSE' ? 'budgets.expenses' : 'budgets.history') }))}
            />
          ) : null}
          <div className="tx-pane wide budget-view" {...tabPanelProps(tab)} ref={paneRef}>
            {tab === 'EXPENSE' ? half : null}
            {tab === 'HISTORY' ? historyList : null}
          </div>
        </>
      ) : (
        <div className="tx-pane wide budget-view" ref={paneRef}>
          {half}
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
