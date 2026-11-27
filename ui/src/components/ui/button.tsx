import * as React from 'react'
import type { VariantProps } from 'class-variance-authority'
import { Slot } from 'radix-ui'

import { buttonVariants, cn } from './cn'

/** A button in one of the looks of the design. `type` is "button" unless said otherwise, so a button in a form does not submit it. */
function Button({
  className,
  variant = 'default',
  size = 'default',
  type = 'button',
  asChild = false,
  ...props
}: React.ComponentProps<'button'> & VariantProps<typeof buttonVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : 'button'
  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      {...(asChild ? {} : { type })}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  )
}

export { Button }
