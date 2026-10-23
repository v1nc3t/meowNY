// Small helpers for the ui/ components. Their looks live in styles/primitives.css, every class starting with "ui-".

/** Joins class names, leaving out the ones that are off (false, null, undefined or empty). */
export function cn(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(' ')
}

export type ButtonVariant = 'default' | 'secondary' | 'outline' | 'ghost' | 'destructive' | 'link'
export type ButtonSize = 'default' | 'sm' | 'lg' | 'icon'

/** The classes of a button, for something that is not a <button> but should look like one (a link). */
export function buttonClass(variant: ButtonVariant = 'default', size: ButtonSize = 'default'): string {
  return cn('ui-btn', `ui-btn-${variant}`, size !== 'default' && `ui-btn-${size}`)
}
