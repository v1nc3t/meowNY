import type { PointerEvent } from 'react'
import { useT } from '../../i18n/context'
import type { Widget } from '../../lib/dashboardLayout'
import type { Profile } from '../../lib/dashboardWidgets'
import Icon from '../NavIcon'
import { Button } from '../ui/button'
import SizePicker from './SizePicker'

/**
 * What a widget shows in edit mode, over its top and its corner: the handle to drag it by, its name, the size button, the settings button, the
 * remove button, and the handle at its bottom right to resize it by dragging. The dragging itself is done by the grid.
 */
export default function WidgetControls({
  widget,
  name,
  profile,
  onMoveStart,
  onResizeStart,
  onSize,
  onSettings,
  onRemove,
}: {
  widget: Widget
  name: string
  profile: Profile
  onMoveStart: (event: PointerEvent<HTMLElement>) => void
  onResizeStart: (event: PointerEvent<HTMLElement>) => void
  onSize: (w: number, h: number) => void
  onSettings: () => void
  onRemove: () => void
}) {
  const t = useT()
  return (
    <>
      <div className="dash-controls">
        <button type="button" className="dash-handle" aria-label={t('dashboard.moveWidget', { name })} title={t('dashboard.moveWidget', { name })} onPointerDown={onMoveStart}>
          <Icon name="grip" className="btn-icon" />
        </button>
        <span className="dash-controls-name">{name}</span>
        <SizePicker type={widget.type} profile={profile} w={widget.w} h={widget.h} name={name} onPick={onSize} />
        <Button variant="ghost" size="icon" className="icon-btn" aria-label={t('dashboard.settingsOf', { name })} title={t('dashboard.settingsOf', { name })} onClick={onSettings}>
          <Icon name="settings" className="btn-icon" />
        </Button>
        <Button variant="ghost" size="icon" className="icon-btn" aria-label={t('dashboard.removeWidget', { name })} title={t('dashboard.removeWidget', { name })} onClick={onRemove}>
          <Icon name="trash" className="btn-icon" />
        </Button>
      </div>
      <span className="dash-resize" role="presentation" aria-hidden="true" title={t('dashboard.resizeWidget', { name })} onPointerDown={onResizeStart} />
    </>
  )
}
