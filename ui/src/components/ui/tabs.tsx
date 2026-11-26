import * as React from 'react'
import { Tabs as TabsPrimitive } from 'radix-ui'

import { cn } from '@/lib/utils'

const Tabs = TabsPrimitive.Root

// "ui-tabs" is a name for the page style sheets (a dialog's tabs fill its width, a phone's are taller); nothing styles it.

/** A grey track, its tabs equally wide. */
function TabsList({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.List>) {
  return (
    <TabsPrimitive.List
      data-slot="tabs-list"
      className={cn('ui-tabs inline-grid h-(--control) w-fit max-w-full auto-cols-fr grid-flow-col items-stretch bg-muted p-[0.125rem] text-muted-foreground', className)}
      {...props}
    />
  )
}

/** The chosen tab is flat on the page's colour and marked with a green line below. */
function TabsTrigger({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Trigger>) {
  return (
    <TabsPrimitive.Trigger
      data-slot="tabs-trigger"
      className={cn(
        'min-h-0 min-w-0 overflow-hidden border-0 bg-transparent px-3 py-0 text-sm text-ellipsis whitespace-nowrap text-muted-foreground hover:text-foreground data-active:bg-background data-active:text-foreground data-active:tab-mark',
        className,
      )}
      {...props}
    />
  )
}

export { Tabs, TabsList, TabsTrigger }
