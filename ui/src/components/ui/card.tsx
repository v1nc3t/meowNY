import * as React from 'react'

import { cn } from '@/lib/utils'

// A card is a header (a title, and an action in its top right corner) and a content area.

function Card({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="card"
      className={cn('flex min-w-0 flex-col gap-(--space-4) border-hair border-border bg-card p-(--space-5) phone:p-(--space-4) text-card-foreground', className)}
      {...props}
    />
  )
}

function CardHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot="card-header" className={cn('grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-4', className)} {...props} />
}

/** The title is in the pixel font. A link in it reads as the title. */
function CardTitle({ className, ...props }: React.ComponentProps<'h2'>) {
  return (
    <h2
      data-slot="card-title"
      className={cn(
        'col-start-1 m-0 font-pixel text-card-foreground [&_a]:text-inherit [&_a]:no-underline [&_a]:underline-offset-[0.25em] [&_a:focus-visible]:underline [&_a:hover]:underline',
        className,
      )}
      {...props}
    />
  )
}

/** Sits in the top right corner of the header, beside the title. */
function CardAction({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="card-action"
      className={cn('col-start-2 row-span-2 row-start-1 flex items-center gap-2 self-start justify-self-end', className)}
      {...props}
    />
  )
}

function CardContent({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot="card-content" className={cn('min-w-0', className)} {...props} />
}

export { Card, CardHeader, CardTitle, CardAction, CardContent }
