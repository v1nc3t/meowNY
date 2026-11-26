import * as React from 'react'
import { Dialog as SheetPrimitive } from 'radix-ui'

import { cn } from '@/lib/utils'

const Sheet = SheetPrimitive.Root

/**
 * A panel that slides in from one side over a dimmed page, as tall as the screen. Like the dialog it is a `<dialog open>`, so the app's own
 * rules for dialogs (arrow keys, shortcuts, focus) apply to it. The slide takes --t-fast, and does not move with less motion (base.css).
 */
function SheetContent({
  className,
  children,
  side = 'left',
  ref,
  ...props
}: Omit<React.ComponentProps<typeof SheetPrimitive.Content>, 'ref'> & { side?: 'left' | 'right'; ref?: React.Ref<HTMLDialogElement> }) {
  return (
    <SheetPrimitive.Portal>
      <SheetPrimitive.Overlay
        data-slot="sheet-overlay"
        className="fixed inset-0 z-50 bg-(--overlay) data-closed:animate-[fade-out_var(--t-fast)_linear_forwards] data-open:animate-[fade-in_var(--t-fast)_linear]"
      />
      <SheetPrimitive.Content asChild {...props}>
        <dialog
          ref={ref}
          open
          data-slot="sheet-content"
          data-side={side}
          className={cn(
            'fixed inset-y-0 z-50 m-0 h-dvh max-h-none w-[min(80vw,21.5rem)] max-w-none overflow-hidden border-solid border-border bg-sidebar p-0 text-foreground',
            side === 'left'
              ? 'right-auto left-0 [border-width:0_var(--hairline)_0_0] data-closed:animate-[sheet-out-left_var(--t-fast)_ease-in_forwards] data-open:animate-[sheet-in-left_var(--t-fast)_ease-out]'
              : 'right-0 left-auto [border-width:0_0_0_var(--hairline)] data-closed:animate-[sheet-out-right_var(--t-fast)_ease-in_forwards] data-open:animate-[sheet-in-right_var(--t-fast)_ease-out]',
            className,
          )}
        >
          {children}
        </dialog>
      </SheetPrimitive.Content>
    </SheetPrimitive.Portal>
  )
}

/** The name of the panel (the caller hides it from the eye with the app's own "sr-only"). */
function SheetTitle(props: React.ComponentProps<typeof SheetPrimitive.Title>) {
  return <SheetPrimitive.Title data-slot="sheet-title" {...props} />
}

export { Sheet, SheetContent, SheetTitle }
