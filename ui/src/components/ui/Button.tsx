import type { ComponentProps } from 'react'
import { buttonClass, cn, type ButtonSize, type ButtonVariant } from './cn'

/** A button in one of the looks of the design. Plain <button> elements are the ghost look already; this one picks another. */
export default function Button({
  variant = 'default',
  size = 'default',
  type = 'button',
  className,
  ...rest
}: ComponentProps<'button'> & { variant?: ButtonVariant; size?: ButtonSize }) {
  return <button type={type} className={cn(buttonClass(variant, size), className)} {...rest} />
}
