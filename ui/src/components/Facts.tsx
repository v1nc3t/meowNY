import { Fragment, type ReactNode } from 'react'
import { cn } from './ui/cn'

/** One pair of `Facts`: a label and its value. `tone` colours the value: green, red, or quiet grey. */
export type FactItem = { label: string; value: ReactNode; tone?: 'positive' | 'negative' | 'muted' }

/**
 * Label and value pairs in two columns: the label at the left in small grey, the value at the right (an amount keeps its tabular figures,
 * and a value that does not fit is cut with an ellipsis). The details of a row, or of a page's thing. A pair with no value (null,
 * undefined, false or an empty string) is left out, so a caller can pass optional facts inline: `{ label: 'rate', value: rate }`; so is
 * a `false` or `null` in place of a pair (`active && { label: …, value: … }`). With nothing to show it draws nothing.
 */
export default function Facts({ items, className }: { items: readonly (FactItem | false | null | undefined)[]; className?: string }) {
  const shown = items.filter((item): item is FactItem => !!item && item.value != null && item.value !== false && item.value !== '')
  if (shown.length === 0) return null
  return (
    <dl className={cn('facts', className)}>
      {shown.map((item) => (
        <Fragment key={item.label}>
          <dt>{item.label}</dt>
          <dd className={item.tone ? `is-${item.tone}` : undefined}>{item.value}</dd>
        </Fragment>
      ))}
    </dl>
  )
}
