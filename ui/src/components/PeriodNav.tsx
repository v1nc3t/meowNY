import { useEffect, useState, type KeyboardEvent } from 'react'
import { createPortal } from 'react-dom'
import { monthNames, shiftMonth, yearChoices } from '../api/months'
import { usePopup } from '../hooks/usePopup'
import { useI18n } from '../i18n/context'
import { tip } from '../keys/tip'
import { Button } from './ui/button'
import { cn } from './ui/cn'

/**
 * One compact control for a month or a year: "‹ nov 2026 ›". The arrows step; the middle opens a small popover to choose the month (with
 * its year) or the year; "current" shows only while another period than the current one is shown. `value` and `current` are "YYYY-MM" for
 * a month and "YYYY" for a year. `keys` marks the arrows and "current" with the keys `[`, `]` and `t` (the page binds them).
 */
export default function PeriodNav({
  kind,
  value,
  current,
  onChange,
  onCurrent,
  prevLabel,
  nextLabel,
  keys = false,
  className,
}: {
  kind: 'month' | 'year'
  value: string
  current: string
  onChange: (value: string) => void
  /** What "current" does, when it is not `onChange(current)`. */
  onCurrent?: () => void
  prevLabel: string
  nextLabel: string
  keys?: boolean
  className?: string
}) {
  const { t, language } = useI18n()
  const names = monthNames(language)
  const { open, host, openPopup, close, buttonRef, popupRef, onKeyDown } = usePopup('start')
  const year = Number(value.slice(0, 4))
  const monthAt = Number(value.slice(5, 7))
  const label = kind === 'month' ? `${names[monthAt - 1] ?? ''} ${year}` : String(year)
  const step = (delta: number) => onChange(kind === 'month' ? shiftMonth(value, delta) : String(year + delta))
  // the year the popover shows: it steps without choosing; opening it shows the year of the value
  const [shown, setShown] = useState(year)

  useEffect(() => {
    if (open) popupRef.current?.querySelector<HTMLElement>('[aria-pressed="true"]')?.focus()
  }, [open, popupRef])

  function choose(next: string) {
    onChange(next)
    close()
  }

  // the arrows move over the grid of choices, four to a row
  function onGridKey(event: KeyboardEvent<HTMLDivElement>) {
    const moves: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -4, ArrowDown: 4 }
    if (!(event.key in moves) || event.altKey || event.ctrlKey || event.metaKey) return
    const cells = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('.period-cell'))
    const at = cells.indexOf(document.activeElement as HTMLElement)
    if (at < 0) return
    event.preventDefault()
    cells[Math.max(0, Math.min(cells.length - 1, at + moves[event.key]))]?.focus()
  }

  const years = yearChoices(`${year}`, Number(current.slice(0, 4)))

  return (
    <div className={cn('period-nav', className)}>
      <Button variant="ghost" size="icon" aria-label={prevLabel} {...(keys ? tip('prev') : {})} onClick={() => step(-1)}>
        &lsaquo;
      </Button>
      <Button
        ref={buttonRef}
        variant="ghost"
        className="period-pick"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          if (open) close()
          else {
            setShown(year)
            openPopup()
          }
        }}
      >
        {label}
      </Button>
      <Button variant="ghost" size="icon" aria-label={nextLabel} {...(keys ? tip('next') : {})} onClick={() => step(1)}>
        &rsaquo;
      </Button>
      {value === current ? null : (
        <Button variant="outline" size="sm" className="period-current" {...(keys ? tip('today') : {})} onClick={() => (onCurrent ? onCurrent() : onChange(current))}>
          {t('common.current')}
        </Button>
      )}
      {host
        ? createPortal(
            <div ref={popupRef} className="period-popover" role="dialog" aria-label={kind === 'month' ? t('common.month') : t('common.year')} onKeyDown={onKeyDown}>
              {kind === 'month' ? (
                <div className="period-years">
                  <Button variant="ghost" size="icon" aria-label={t('dashboard.prevYear')} onClick={() => setShown((current) => current - 1)}>
                    &lsaquo;
                  </Button>
                  <span aria-live="polite">{shown}</span>
                  <Button variant="ghost" size="icon" aria-label={t('dashboard.nextYear')} onClick={() => setShown((current) => current + 1)}>
                    &rsaquo;
                  </Button>
                </div>
              ) : null}
              <div className="period-grid" onKeyDown={onGridKey}>
                {kind === 'month'
                  ? names.map((name, at) => {
                      const next = `${shown}-${String(at + 1).padStart(2, '0')}`
                      return (
                        <Button key={name + at} variant="ghost" size="sm" className="period-cell" aria-pressed={next === value} onClick={() => choose(next)}>
                          {name}
                        </Button>
                      )
                    })
                  : years.map((each) => (
                      <Button key={each} variant="ghost" size="sm" className="period-cell" aria-pressed={each === year} onClick={() => choose(String(each))}>
                        {each}
                      </Button>
                    ))}
              </div>
            </div>,
            host,
          )
        : null}
    </div>
  )
}
