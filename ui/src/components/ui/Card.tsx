import type { ComponentProps } from 'react'
import { cn } from './cn'

// A card is a header (title, description and an action in the corner), a content area and a footer, each a thin wrapper that
// only adds its class and passes everything else on.

export function Card({ className, ...rest }: ComponentProps<'div'>) {
  return <div className={cn('ui-card', className)} {...rest} />
}

export function CardHeader({ className, ...rest }: ComponentProps<'div'>) {
  return <div className={cn('ui-card-header', className)} {...rest} />
}

export function CardTitle({ className, ...rest }: ComponentProps<'h2'>) {
  return <h2 className={cn('ui-card-title', className)} {...rest} />
}

export function CardDescription({ className, ...rest }: ComponentProps<'div'>) {
  return <div className={cn('ui-card-description', className)} {...rest} />
}

/** Sits in the top right corner of the header, beside the title. */
export function CardAction({ className, ...rest }: ComponentProps<'div'>) {
  return <div className={cn('ui-card-action', className)} {...rest} />
}

export function CardContent({ className, ...rest }: ComponentProps<'div'>) {
  return <div className={cn('ui-card-content', className)} {...rest} />
}

export function CardFooter({ className, ...rest }: ComponentProps<'div'>) {
  return <div className={cn('ui-card-footer', className)} {...rest} />
}
