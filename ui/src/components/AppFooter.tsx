import { useLayoutEffect, useRef } from 'react'
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
 * The foot of the settings and of the lock screens: who made the app, its licence and version at the left, the links at the right.
 * Two groups: one line when there is room, else two (a phone always shows two), with a | between the items of a group.
 */
export default function AppFooter() {
  const t = useT()
  const navRef = useRef<HTMLElement>(null)

  // A | belongs to the item after it, so none ends a line; the one before an item that starts a line is hidden. Hiding it takes up
  // no room, so it can never change where the line breaks.
  useLayoutEffect(() => {
    const nav = navRef.current
    if (!nav) return
    const items = [...nav.querySelectorAll('li')]
    const mark = () => {
      let top = NaN
      for (const item of items) {
        item.toggleAttribute('data-first', item.offsetTop !== top)
        top = item.offsetTop
      }
    }
    mark()
    const watch = new ResizeObserver(mark)
    watch.observe(nav)
    items.forEach((item) => watch.observe(item))
    return () => watch.disconnect()
  }, [])

  const groups = [
    [
      { key: 'copyright', text: `meowny © ${YEAR} v1nc3t`, href: REPO },
      { key: 'licence', text: 'AGPL-3.0', href: `${REPO}/blob/main/LICENSE` },
      // the version says where it leads, as its text does not
      { key: 'version', text: `v${__APP_VERSION__}`, href: UPDATE_URL, title: t('settings.linkUpdate') },
    ],
    [
      { key: 'update', text: t('settings.linkUpdate'), href: UPDATE_URL },
      { key: 'discussions', text: t('settings.linkDiscussions'), href: `${REPO}/discussions` },
      { key: 'sponsor', text: t('settings.linkSponsor'), href: 'https://ko-fi.com/v1nc3t' },
    ],
  ]

  return (
    <footer className="app-footer">
      <nav ref={navRef} aria-label={t('settings.linksLabel')}>
        {groups.map((group) => (
          <ul key={group[0].key}>
            {group.map((item) => (
              <li key={item.key}>
                <a href={item.href} title={item.title} target="_blank" rel="noreferrer">
                  {item.text}
                </a>
              </li>
            ))}
          </ul>
        ))}
      </nav>
    </footer>
  )
}
