import { monthNames, shiftMonth, withMonth, withYear, yearChoices } from '../api/months'
import Dropdown from './Dropdown'
import Button from './ui/Button'
import { useI18n } from '../i18n/context'
import { tip } from '../keys/tip'

/** Previous and next month, a month and a year picker, and a button back to the current month. */
export default function MonthNav({
  month,
  thisMonth,
  onChange,
}: {
  month: string
  thisMonth: string
  onChange: (month: string) => void
}) {
  const { t, language } = useI18n()
  return (
    <div className="month-nav">
      <Button variant="outline" size="icon" aria-label={t('budgets.prevMonth')} {...tip('prev')} onClick={() => onChange(shiftMonth(month, -1))}>
        &lsaquo;
      </Button>
      <Dropdown
        compact
        label={t('common.month')}
        value={String(Number(month.slice(5, 7)))}
        options={monthNames(language).map((name, index) => ({ value: String(index + 1), label: name }))}
        onChange={(value) => onChange(withMonth(month, Number(value)))}
      />
      <Dropdown
        compact
        label={t('common.year')}
        value={month.slice(0, 4)}
        options={yearChoices(month, Number(thisMonth.slice(0, 4))).map((year) => ({
          value: String(year),
          label: String(year),
        }))}
        onChange={(value) => onChange(withYear(month, Number(value)))}
      />
      <Button variant="outline" size="icon" aria-label={t('budgets.nextMonth')} {...tip('next')} onClick={() => onChange(shiftMonth(month, 1))}>
        &rsaquo;
      </Button>
      <Button variant="outline" disabled={month === thisMonth} {...tip('today')} onClick={() => onChange(thisMonth)}>
        {t('common.current')}
      </Button>
    </div>
  )
}
