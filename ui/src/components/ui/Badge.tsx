import type { ComponentProps } from 'react'
import { cn } from './cn'

/** A small label: a status, a change, a count. */
export default function Badge({
  variant,
  className,
  ...rest
}: ComponentProps<'span'> & { variant: 'secondary' | 'outline' | 'destructive' | 'success' }) {
  return <span className={cn('ui-badge', `ui-badge-${variant}`, className)} {...rest} />
}
