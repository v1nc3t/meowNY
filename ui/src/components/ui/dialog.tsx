import * as React from 'react'
import { Dialog as DialogPrimitive } from 'radix-ui'

import { cn } from '@/lib/utils'

const Dialog = DialogPrimitive.Root

/**
 * The window: a dimmed page, and a box in the middle with a thin line and the one big hard shadow. The box is a `<dialog open>`, not a div:
 * the app finds its open dialog by that element (the arrow keys, the focus, the shortcuts, the toasts and the drop-down lists all look
 * for `dialog[open]`). It has no close button; a dialog has its own buttons at the foot.
 */
function DialogContent({
  className,
  children,
  ref,
  ...props
}: Omit<React.ComponentProps<typeof DialogPrimitive.Content>, 'ref'> & { ref?: React.Ref<HTMLDialogElement> }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay data-slot="dialog-overlay" className="fixed inset-0 z-50 bg-(--overlay) data-open:animate-[fade-in_var(--t-fast)_linear]" />
      <DialogPrimitive.Content asChild {...props}>
        <dialog
          ref={ref}
          open
          data-slot="dialog-content"
          className={cn(
            'fixed inset-0 z-50 m-auto h-fit max-h-[calc(100svh-2rem)] w-[min(32rem,calc(100vw-2rem))] overflow-visible border-hair border-border bg-background p-0 text-foreground shadow-float data-open:animate-[dialog-in_var(--t-fast)_var(--ease-step)]',
            className,
          )}
        >
          {children}
        </dialog>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  )
}

function DialogHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot="dialog-header" className={cn('flex items-center gap-2', className)} {...props} />
}

/** In the pixel font. */
function DialogTitle({ className, ...props }: React.ComponentProps<typeof DialogPrimitive.Title>) {
  return <DialogPrimitive.Title data-slot="dialog-title" className={cn('m-0 font-pixel text-foreground', className)} {...props} />
}

function DialogFooter({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot="dialog-footer" className={cn('flex flex-nowrap items-center justify-end gap-2', className)} {...props} />
}

export { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter }
