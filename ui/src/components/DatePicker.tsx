import { changePart, daysInMonth, joinParts, monthNames, parseParts, yearChoices, type DateKind } from '../api/months'
import Dropdown from './Dropdown'
import { useI18n } from '../i18n/context'

/**
 * A date as dropdowns: day, month and year for a date, month and year for a month, year alone for a year.
 * `today` ("YYYY-MM-DD") is what an empty or invalid value shows, and centres the list of years.
 * With `name`, a hidden input carries the value so a form submit includes it.
 */
export default function DatePicker({
  kind,
  value,
  onChange,
  label,
  today,
  name,
}: {
  kind: DateKind
  value: string
  onChange: (value: string) => void
  label: string
  today: string
  name?: string
}) {
  const { t, language } = useI18n()
  const parts = parseParts(kind, value, today)
  const update = (part: 'day' | 'month' | 'year', next: string) =>
    onChange(joinParts(kind, changePart(parts, part, Number(next))))

  return (
    <div className="date-picker">
      {kind === 'date' ? (
        <Dropdown
          compact
          label={`${label} ${t('common.day')}`}
          value={String(parts.day)}
          options={Array.from({ length: daysInMonth(parts.year, parts.month) }, (_, index) => ({
            value: String(index + 1),
            label: String(index + 1).padStart(2, '0'),
          }))}
          onChange={(next) => update('day', next)}
        />
      ) : null}
      {kind !== 'year' ? (
        <Dropdown
          compact
          label={`${label} ${t('common.month')}`}
          value={String(parts.month)}
          options={monthNames(language).map((month, index) => ({ value: String(index + 1), label: month }))}
          onChange={(next) => update('month', next)}
        />
      ) : null}
      <Dropdown
        compact
        label={`${label} ${t('common.year')}`}
        value={String(parts.year)}
        options={yearChoices(`${parts.year}`, Number(today.slice(0, 4))).map((year) => ({
          value: String(year),
          label: String(year),
        }))}
        onChange={(next) => update('year', next)}
      />
      {name ? <input type="hidden" name={name} value={joinParts(kind, parts)} /> : null}
    </div>
  )
}
