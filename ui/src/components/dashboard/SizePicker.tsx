import { useState, type KeyboardEvent } from 'react'
import { useT } from '../../i18n/context'
import { COLUMNS, MAX_ROWS, sizeRules, type Profile, type WidgetType } from '../../lib/dashboardWidgets'
import { Button } from '../ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover'

/**
 * The size button of a widget: it shows the size ("6×2") and opens a small grid of cells, one for each size the grid has (12 by 4 on the
 * desktop, 2 by 4 on the phone). The sizes the widget can take are lit and the others dimmed. Hover, or the arrow keys, show how much the widget
 * would take; a click, or Enter, sets it.
 */
export default function SizePicker({
  type,
  profile,
  w,
  h,
  name,
  onPick,
}: {
  type: WidgetType
  profile: Profile
  w: number
  h: number
  name: string
  onPick: (w: number, h: number) => void
}) {
  const t = useT()
  const rules = sizeRules(type, profile)
  const columns = COLUMNS[profile]
  const [open, setOpen] = useState(false)
  const [preview, setPreview] = useState({ w, h })
  const fits = (cw: number, ch: number) => rules.widths.includes(cw) && ch >= rules.minH && ch <= rules.maxH

  function choose(cw: number, ch: number) {
    if (!fits(cw, ch)) return
    setOpen(false)
    onPick(cw, ch)
  }

  function onKeyDown(event: KeyboardEvent) {
    const wider = rules.widths.filter((each) => each > preview.w)[0]
    const narrower = rules.widths.filter((each) => each < preview.w).at(-1)
    const next = {
      ArrowRight: wider === undefined ? preview : { ...preview, w: wider },
      ArrowLeft: narrower === undefined ? preview : { ...preview, w: narrower },
      ArrowDown: { ...preview, h: Math.min(rules.maxH, preview.h + 1) },
      ArrowUp: { ...preview, h: Math.max(rules.minH, preview.h - 1) },
    }[event.key]
    if (next) setPreview(next)
    else if (event.key === 'Enter' || event.key === ' ') choose(preview.w, preview.h)
    else return
    event.preventDefault()
  }

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (next) setPreview({ w, h })
      }}
    >
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" className="dash-size-button" aria-label={t('dashboard.sizeOf', { name, w, h })}>
          {w}×{h}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="dash-size-pop" aria-label={t('dashboard.size')} align="end">
        <div
          className="dash-size-grid"
          role="group"
          tabIndex={0}
          aria-label={t('dashboard.size')}
          style={{ gridTemplateColumns: `repeat(${columns}, 1fr)` }}
          onKeyDown={onKeyDown}
          onPointerLeave={() => setPreview({ w, h })}
        >
          {Array.from({ length: MAX_ROWS * columns }, (_, at) => {
            const cw = (at % columns) + 1
            const ch = Math.floor(at / columns) + 1
            const on = fits(cw, ch)
            return (
              <span
                key={at}
                className={`dash-size-cell${on ? '' : ' is-off'}${cw <= preview.w && ch <= preview.h ? ' is-in' : ''}${cw === w && ch === h ? ' is-now' : ''}`}
                onPointerEnter={() => on && setPreview({ w: cw, h: ch })}
                onClick={() => choose(cw, ch)}
              />
            )
          })}
        </div>
        <p className="dash-size-read" aria-live="polite">
          {t('dashboard.sizeText', { w: preview.w, h: preview.h })}
        </p>
      </PopoverContent>
    </Popover>
  )
}
