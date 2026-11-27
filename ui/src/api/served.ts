// The app served to a browser by `meowny-server` (docs/self-host.md): the same commands as the desktop app, sent over HTTP. The
// browser signs in with the passphrase and keeps a session cookie the page cannot read. Where the desktop opens a file dialog, the
// browser downloads the file the command wrote, or uploads the file the person picks before the command runs.

/** True in a page built for `meowny-server` (`MEOWNY_SERVED=1 npm run build`). */
export function isServed(): boolean {
  return typeof __MEOWNY_SERVED__ !== 'undefined' && __MEOWNY_SERVED__
}

/** Fired on the window when the server answers `locked`: the data was locked, or this browser's session has ended. */
export const LOCKED_EVENT = 'meowny-locked'

/** The commands that open a file the person picks, and the kind of file the picker offers. */
const PICKS_FILE: Readonly<Record<string, string>> = {
  restore_backup: '.meowbak',
  restore_backup_new_vault: '.meowbak',
}

/** A request from this page: the header tells the server it is not a form or a link on another site. */
const OURS = { 'X-Meowny': '1' }

export type Answer = { ok: true; data: unknown; download?: { id: string; name: string } } | { ok: false; error: unknown }

/** What an answer of the server gives the caller: the data, or the code it throws. A download, if any, is for `fetchDownload`. */
export function readAnswer(answer: unknown): { data: unknown; download: { id: string; name: string } | null } {
  if (typeof answer !== 'object' || answer === null || !('ok' in answer)) throw 'internal'
  const typed = answer as Answer
  if (!typed.ok) throw typed.error
  const download = typed.download
  const valid = download && typeof download.id === 'string' && /^[0-9a-f]{32}$/.test(download.id) && typeof download.name === 'string'
  return { data: typed.data, download: valid ? download : null }
}

/** Asks the browser for one file. Null when the person closes the picker. */
function pickFile(accept: string): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = accept
    input.addEventListener('change', () => resolve(input.files?.[0] ?? null), { once: true })
    input.addEventListener('cancel', () => resolve(null), { once: true })
    input.click()
  })
}

async function upload(file: File): Promise<string> {
  const response = await fetch('/api/upload', { method: 'POST', headers: { ...OURS, 'Content-Type': 'application/octet-stream' }, body: file })
  const { data } = readAnswer(await response.json())
  if (typeof data !== 'string') throw 'internal'
  return data
}

/** Hands the file a command wrote to the browser, which saves it as it saves any download. */
function fetchDownload(download: { id: string; name: string }): void {
  const link = document.createElement('a')
  link.href = `/api/download/${download.id}`
  link.download = download.name
  link.rel = 'noopener'
  document.body.append(link)
  link.click()
  link.remove()
}

/** One command, as `invoke` sends it to the desktop app. Throws the error code. */
export async function servedCall(command: string, args?: Record<string, unknown>): Promise<unknown> {
  const headers: Record<string, string> = { ...OURS, 'Content-Type': 'application/json' }
  const accept = PICKS_FILE[command]
  if (accept !== undefined) {
    // no file is the same as a cancelled dialog: the command answers that nothing was done
    const file = await pickFile(accept)
    if (file) headers['X-Meowny-Upload'] = await upload(file)
  }
  const response = await fetch(`/api/invoke/${command}`, { method: 'POST', headers, body: JSON.stringify(args ?? {}) })
  let answer: ReturnType<typeof readAnswer>
  try {
    answer = readAnswer(await response.json())
  } catch (error) {
    if (error === 'locked') window.dispatchEvent(new Event(LOCKED_EVENT))
    throw error
  }
  if (answer.download) fetchDownload(answer.download)
  return answer.data
}

/** How often a served page asks whether the data is still open, so a lock elsewhere (the idle timer) shows the lock screen here. */
const LOCK_CHECK_MS = 30_000

/** Calls `handler` when the data is locked for this browser. Returns the way to stop. */
export function onServedLock(handler: () => void, status: () => Promise<string>): () => void {
  const fire = () => handler()
  window.addEventListener(LOCKED_EVENT, fire)
  const timer = window.setInterval(() => {
    status().then((now) => now !== 'unlocked' && handler(), () => undefined)
  }, LOCK_CHECK_MS)
  return () => {
    window.removeEventListener(LOCKED_EVENT, fire)
    window.clearInterval(timer)
  }
}
