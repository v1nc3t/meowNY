import * as React from 'react'
import { Popover as PopoverPrimitive } from 'radix-ui'

import { cn } from '@/lib/utils'

const Popover = PopoverPrimitive.Root
const PopoverTrigger = PopoverPrimitive.Trigger

/** A small box of text over the page, with a thin line and a small hard shadow. */
function PopoverContent({ className, align = 'start', sideOffset = 4, ...props }: React.ComponentProps<typeof PopoverPrimitive.Content>) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        data-slot="popover-content"
        align={align}
        sideOffset={sideOffset}
        collisionPadding={8}
        className={cn(
          'z-60 w-max max-w-[min(20rem,calc(100vw-1rem))] border-hair border-input bg-popover px-4 py-3.5 text-[0.8125rem] leading-normal text-popover-foreground shadow-pop outline-none data-open:animate-[panel-in_var(--t-fast)_var(--ease-step)]',
          className,
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  )
}

export { Popover, PopoverTrigger, PopoverContent }
