import { vault } from '../vault/api'

/** Locks the app and shows the lock screen again (the page is loaded anew, so nothing of the open vault is left in it). */
export function lockApp(): void {
  void vault.lock().then(() => window.location.reload(), () => undefined)
}
