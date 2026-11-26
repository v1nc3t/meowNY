import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

/** Joins class names (clsx: the ones that are off, false, null or empty, are left out) and lets the last of two Tailwind classes that set the same thing win. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs))
}
