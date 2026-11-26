import { Fragment, type ReactNode } from 'react'
import { cn } from './ui/cn'

/** One pair of `Facts`: a label and its value. `tone` colours the value: green, red, or quiet grey. */
export type FactItem = { label: string; value: ReactNode; tone?: 'positive' | 'negative' | 'muted' }

/**
 * Label and value pairs in two columns: the label at the left in small grey, the value at the right (an amount keeps its tabular figures,
 * and a value that does not fit is cut with an ellipsis). The details of a row, or of a page's thing. A pair with no value (null,
 * undefined, false or an empty string) is left out, so a caller can pass optional facts inline: `{ label: 'rate', value: rate }`; so is
 * a `false` or `null` in place of a pair (`active && { label: …, value: … }`). With nothing to show it draws nothing.
 * `wide` is the form for the page of a thing on a computer: the pairs fill the whole width of the page, each a small grey label over its value, in as
 * many columns as fit (2 to 4), instead of one narrow column.
 */
export default function Facts({ items, className, wide = false }: { items: readonly (FactItem | false | null | undefined)[]; className?: string; wide?: boolean }) {
  const shown = items.filter((item): item is FactItem => !!item && item.value != null && item.value !== false && item.value !== '')
  if (shown.length === 0) return null
  if (wide) {
    return (
      <dl className={cn('facts is-wide', className)}>
        {shown.map((item) => (
          <div key={item.label}>
            <dt>{item.label}</dt>
            <dd className={item.tone ? `is-${item.tone}` : undefined}>{item.value}</dd>
          </div>
        ))}
      </dl>
    )
  }
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

/** One column of a `FactStrip`. */
export type StripItem = { label: string; value: ReactNode; tone?: 'negative' }

/**
 * A few values side by side, each with its label over it: the totals strip of the transactions page, for a live line in a dialog (what a
 * payment does to a debt, what a stash holds after a change). Up to three: the first at the left, the last at the right. `tone` colours the
 * value red (`negative`).
 */
export function FactStrip({ items }: { items: readonly (StripItem | false | null | undefined)[] }) {
  return (
    <div className="tx-total is-strip">
      <span className="tx-sums">
        {items.filter((item) => !!item).map((item) => (
          <span key={item.label} className="tx-sum">
            <span className="tx-sum-label">{item.label}</span>
            <span className={item.tone ? 'is-expense' : undefined}>{item.value}</span>
          </span>
        ))}
      </span>
    </div>
  )
}
