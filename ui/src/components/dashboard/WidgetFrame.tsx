import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { useT } from '../../i18n/context'
import { SPECS } from '../../lib/dashboardWidgets'
import { Card, CardAction, CardContent, CardHeader, CardTitle } from '../ui/card'
import { cn } from '../ui/cn'
import { useWidget } from './context'

/** One look for every widget: a title line (the one the person gave it, else `name` (a widget of one account, category or stash is named by it), else
 * the widget's own name; a link to the page when `to` is given, and a value or a link at the right as `aside`), and the body, which takes the rest of
 * the widget and scrolls when it has more than fits. */
export default function WidgetFrame({ to, aside, name, className, children }: { to?: string; aside?: ReactNode; name?: string; className?: string; children?: ReactNode }) {
  const t = useT()
  const { widget } = useWidget()
  const title = widget.settings.title ?? (name || t(SPECS[widget.type].name))
  return (
    <Card className="dash-widget" data-widget={widget.type}>
      <CardHeader className="dash-widget-head">
        <CardTitle className="dash-widget-title">{to ? <Link to={to}>{title}</Link> : title}</CardTitle>
        {aside ? <CardAction>{aside}</CardAction> : null}
      </CardHeader>
      <CardContent className={cn('dash-widget-body', className)}>{children}</CardContent>
    </Card>
  )
}

/** The one short line of a widget with nothing to show. */
export function Empty({ text }: { text: string }) {
  return <p className="dash-empty">{text}</p>
}
