/** Returns the URL when a link should open in the system browser (https only), else null. */
export function externalHref(href: string | null): string | null {
  if (!href) return null
  try {
    const url = new URL(href, 'tauri://localhost')
    return url.protocol === 'https:' ? url.href : null
  } catch {
    return null
  }
}
