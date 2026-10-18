const NAME_KEY = 'bill-sender-name'

/** The name this person gives on the code of a shared bill: asked at the first run, changed in the settings, kept on this device. */
export function senderName(): string {
  try {
    return localStorage.getItem(NAME_KEY) ?? ''
  } catch {
    return ''
  }
}

export function saveSenderName(name: string): void {
  try {
    localStorage.setItem(NAME_KEY, name)
  } catch {
    // Not kept: it is asked again when a code is shown.
  }
}
