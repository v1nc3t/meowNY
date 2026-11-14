import { parseYear, withYear, yearChoices } from '../api/months'
import { useT } from '../i18n/context'
import Dropdown from './Dropdown'
import Button from './ui/Button'

/** Previous and next year around a year picker, and a button back to this year. Keeps the month, so March 2026 becomes March 2025. */
export default function YearNav({ month, thisMonth, onChange }: { month: string; thisMonth: string; onChange: (month: string) => void }) {
  const t = useT()
  const year = Number(month.slice(0, 4))
  return (
    <div className="month-nav dash-year-nav">
      <Button variant="ghost" size="icon" aria-label={t('dashboard.prevYear')} onClick={() => onChange(withYear(month, year - 1))}>
        &lsaquo;
      </Button>
      <Dropdown
        compact
        label={t('common.year')}
        value={String(year)}
        options={yearChoices(month, Number(thisMonth.slice(0, 4))).map((y) => ({ value: String(y), label: String(y) }))}
        parse={(text) => parseYear(text)?.toString() ?? null}
        onChange={(value) => onChange(withYear(month, Number(value)))}
      />
      <Button variant="ghost" size="icon" aria-label={t('dashboard.nextYear')} onClick={() => onChange(withYear(month, year + 1))}>
        &rsaquo;
      </Button>
      <Button variant="outline" disabled={month.slice(0, 4) === thisMonth.slice(0, 4)} onClick={() => onChange(thisMonth)}>
        {t('common.current')}
      </Button>
    </div>
  )
}
