import * as React from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { Slot } from 'radix-ui'

import { cn } from '@/lib/utils'

// "ui-badge" is a name for the page style sheets (settings.css); nothing styles it.
const badgeVariants = cva(
  'ui-badge inline-flex w-fit items-center gap-1 border-hair px-2 py-0.5 text-xs leading-[1.4] font-semibold whitespace-nowrap',
  {
    variants: {
      variant: {
        default: 'border-primary bg-primary text-primary-foreground',
        secondary: 'border-transparent bg-secondary text-secondary-foreground',
        outline: 'border-border text-foreground',
        destructive: 'border-destructive text-destructive',
        success: 'border-success text-success',
      },
    },
    defaultVariants: { variant: 'default' },
  },
)

/** A small label: a status, a change, a count. The coloured ones are outlined in their colour, so a row of them stays calm. */
function Badge({
  className,
  variant = 'default',
  asChild = false,
  ...props
}: React.ComponentProps<'span'> & VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : 'span'
  return <Comp data-slot="badge" data-variant={variant} className={cn(badgeVariants({ variant }), className)} {...props} />
}

export { Badge }
