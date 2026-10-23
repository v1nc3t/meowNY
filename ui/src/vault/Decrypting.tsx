import { useT } from '../i18n/context'

/**
 * The loading screen after a password: a line that fills, with "decrypting..." under it. It only moves with CSS. A screen reader
 * reads the word and an ellipsis once; the three animated dots are hidden from it.
 */
export default function Decrypting() {
  const t = useT()
  return (
    <div className="decrypting" role="status" aria-live="polite">
      <span className="decrypting-line" aria-hidden="true">
        <span className="decrypting-fill" />
      </span>
      <p className="decrypting-text">
        {t('lock.decrypting')}
        <span className="sr-only">...</span>
        <span className="decrypting-dots" aria-hidden="true">
          ...
        </span>
      </p>
    </div>
  )
}

/** The same on a page of its own, in the frame of the other password screens. */
export function DecryptingPage() {
  return (
    <main className="gate">
      <Decrypting />
    </main>
  )
}
