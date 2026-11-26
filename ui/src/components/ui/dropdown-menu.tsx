import * as React from 'react'
import { DropdownMenu as DropdownMenuPrimitive } from 'radix-ui'

import NavIcon from '@/components/NavIcon'
import { cn } from '@/lib/utils'

const DropdownMenu = DropdownMenuPrimitive.Root
const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger

/** The menu: a list under its button, with a thin line and a small hard shadow. */
function DropdownMenuContent({ className, align = 'end', sideOffset = 4, ...props }: React.ComponentProps<typeof DropdownMenuPrimitive.Content>) {
  return (
    <DropdownMenuPrimitive.Portal>
      <DropdownMenuPrimitive.Content
        data-slot="dropdown-menu-content"
        align={align}
        sideOffset={sideOffset}
        collisionPadding={8}
        className={cn(
          'z-60 flex min-w-48 max-w-[calc(100vw-1rem)] flex-col gap-0.5 overflow-y-auto border-hair border-input bg-popover p-1 text-popover-foreground shadow-pop outline-none data-open:animate-[panel-in_var(--t-fast)_var(--ease-step)]',
          className,
        )}
        {...props}
      />
    </DropdownMenuPrimitive.Portal>
  )
}

// One line: 1.625rem tall (taller on a phone), the one that has the focus or the pointer filled in the quiet fill.
const item =
  'relative flex min-h-(--control) w-full cursor-default items-center gap-2.5 px-3 text-sm whitespace-nowrap outline-none select-none focus:bg-accent focus:text-accent-foreground focus-visible:outline-inset data-disabled:pointer-events-none data-disabled:opacity-50 phone:min-h-[2.625rem] phone:text-base'

function DropdownMenuItem({
  className,
  variant = 'default',
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Item> & { variant?: 'default' | 'destructive' }) {
  return (
    <DropdownMenuPrimitive.Item
      data-slot="dropdown-menu-item"
      data-variant={variant}
      className={cn(item, 'data-[variant=destructive]:text-destructive', className)}
      {...props}
    />
  )
}

/** A switch: the words, and a pixel check mark at the right while it is on (the room of the mark is kept). */
function DropdownMenuCheckboxItem({ className, children, ...props }: React.ComponentProps<typeof DropdownMenuPrimitive.CheckboxItem>) {
  return (
    <DropdownMenuPrimitive.CheckboxItem data-slot="dropdown-menu-checkbox-item" className={cn(item, className)} {...props}>
      {children}
      <span className="ml-auto inline-flex size-(--icon) shrink-0 text-primary" aria-hidden="true">
        <DropdownMenuPrimitive.ItemIndicator>
          <NavIcon name="check" className="btn-icon" />
        </DropdownMenuPrimitive.ItemIndicator>
      </span>
    </DropdownMenuPrimitive.CheckboxItem>
  )
}

export { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuCheckboxItem }
