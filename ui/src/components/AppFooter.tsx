import { useT } from '../i18n/context'

const REPO = 'https://github.com/v1nc3t/meowNY'
const YEAR = new Date().getFullYear()

// The person opens these in the browser; the app makes no request.
const UPDATE_URL = `${REPO}/releases/latest`

/** "check for updates", to follow a message that asks for an update. */
export function UpdateLink() {
  const t = useT()
  return (
    <a href={UPDATE_URL} target="_blank" rel="noreferrer">
      {t('settings.linkUpdate')}
    </a>
  )
}

/**
 * The foot of the settings and of the lock screens: what the app is (its version, licence and copyright) at the left, the links at the
 * right, as small quiet text. The items keep a gap between them and nothing else; the version is text, "check for updates" is the link.
 */
export default function AppFooter() {
  const t = useT()
  return (
    <footer className="app-footer">
      <p>
        <span>meowny v{__APP_VERSION__}</span>
        <a href={`${REPO}/blob/main/LICENSE`} target="_blank" rel="noreferrer">
          AGPL-3.0
        </a>
        <a href={REPO} target="_blank" rel="noreferrer">
          © {YEAR} v1nc3t
        </a>
      </p>
      <nav aria-label={t('settings.linksLabel')}>
        <UpdateLink />
        <a href={`${REPO}/discussions`} target="_blank" rel="noreferrer">
          {t('settings.linkDiscussions')}
        </a>
        <a href="https://ko-fi.com/v1nc3t" target="_blank" rel="noreferrer">
          {t('settings.linkSponsor')}
        </a>
      </nav>
    </footer>
  )
}
