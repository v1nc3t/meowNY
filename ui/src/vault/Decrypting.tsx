import { useT } from '../i18n/context'
import { MIN_SHOW_MS, useFillHeld } from './unlockFlow'

/**
 * The loading line after a password: a line that fills, with "decrypting..." under it ("creating..." while the data is first made). `below` puts it at the foot of the password
 * screen, which stays on show, so a wrong passphrase only has to say so. The line fills once (see useFillHeld); `done` lets it reach its
 * end. A screen reader reads the word and an ellipsis once; the three animated dots are hidden from it.
 */
export default function Decrypting({ below = false, done = false, creating = false }: { below?: boolean; done?: boolean; creating?: boolean }) {
  const t = useT()
  const held = useFillHeld(done)
  return (
    <div className={below ? 'decrypting is-below' : 'decrypting'} role="status" aria-live="polite">
      <span className="decrypting-line" aria-hidden="true">
        <span
          className="decrypting-fill"
          style={{ animationDuration: `${MIN_SHOW_MS}ms`, animationPlayState: held ? 'paused' : 'running' }}
        />
      </span>
      <p className="decrypting-text">
        {t(creating ? 'lock.creating' : 'lock.decrypting')}
        <span className="sr-only">...</span>
        <span className="decrypting-dots" aria-hidden="true">
          ...
        </span>
      </p>
    </div>
  )
}

/** The same on a page of its own, in the frame of the other password screens. */
export function DecryptingPage({ done = false }: { done?: boolean }) {
  return (
    <main className="gate">
      <Decrypting done={done} />
    </main>
  )
}
