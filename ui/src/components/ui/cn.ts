// Small helpers for the ui/ components (shadcn/ui, restyled square and pixel: see styles/tailwind.css).
import { cva, type VariantProps } from 'class-variance-authority'

export { cn } from '@/lib/utils'

// The "ui-btn…" names are styled by nothing: they are what the page style sheets (phone.css, onboarding.css…) call a button, and a link made
// to look like one (`buttonClass`), which has no data attributes.
export const buttonVariants = cva(
  'ui-btn inline-flex min-w-0 shrink-0 items-center justify-center gap-2 border-hair text-sm leading-[1.4] font-normal whitespace-nowrap no-underline select-none cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 disabled:shadow-none [&_svg]:pointer-events-none [&_svg]:shrink-0',
  {
    variants: {
      variant: {
        // The one solid button: it stands on its hard shadow and sinks onto it when pressed (with less motion the shadow just goes).
        default:
          'ui-btn-default border-primary bg-primary text-primary-foreground shadow-press hover:opacity-90 not-disabled:active:shadow-none motion-safe:not-disabled:active:translate-x-(--hairline) motion-safe:not-disabled:active:translate-y-(--hairline)',
        outline:
          'border-input bg-transparent text-foreground shadow-none hover:bg-accent hover:text-accent-foreground motion-safe:not-disabled:active:scale-[0.97]',
        ghost:
          'ui-btn-ghost border-transparent text-foreground shadow-none hover:border-accent hover:bg-accent hover:text-accent-foreground motion-safe:not-disabled:active:scale-[0.97]',
        destructive:
          'border-destructive-solid bg-destructive-solid text-destructive-foreground shadow-none hover:opacity-90 motion-safe:not-disabled:active:scale-[0.97]',
        link: 'border-transparent text-foreground shadow-none underline-offset-[0.25em] hover:underline motion-safe:not-disabled:active:scale-[0.97]',
      },
      size: {
        default: 'h-(--control) px-3.5',
        sm: 'h-(--control-sm) px-3',
        lg: 'h-8 px-5',
        icon: 'ui-btn-icon size-(--control) min-w-(--control) p-0',
      },
    },
    compoundVariants: [{ variant: 'link', className: 'h-auto p-0 text-[0.8125rem]' }],
    defaultVariants: { variant: 'default', size: 'default' },
  },
)

export type ButtonVariant = NonNullable<VariantProps<typeof buttonVariants>['variant']>
export type ButtonSize = NonNullable<VariantProps<typeof buttonVariants>['size']>

/** The classes of a button, for something that is not a <button> but should look like one (a link). */
export function buttonClass(variant: ButtonVariant = 'default', size: ButtonSize = 'default'): string {
  return buttonVariants({ variant, size })
}
