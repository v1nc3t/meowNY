import { Link } from 'react-router-dom'
import type { Stash } from '../api/stashes'
import { useT } from '../i18n/context'
import { figures, nextLine, nextText, progress, stashMoney } from '../lib/stashView'
import { cn } from './ui/cn'

/**
 * How far a stash is: a hairline track with a fill in the accent colour, full and in the success colour once the target is reached. A stash with no
 * target has no bar (its amount says it all); one below 0 is empty. The bar is only a picture: the words next to it say the numbers.
 */
export function StashBar({ saved, target, large = false }: { saved: string; target: string | null; large?: boolean }) {
  if (target === null) return null
  const shown = progress(saved, target)
  return (
    <span className={cn('stash-bar', large && 'is-large', shown.reached && 'is-reached', shown.below && 'is-below')} aria-hidden="true">
      <span className="stash-fill" style={{ width: `${shown.fill * 100}%` }} />
    </span>
  )
}

/**
 * The words of a stash under its bar: "450.00 of 1200.00 · 37%" (or "450.00 saved"), in red when it is below 0, and one line of what is next.
 * The same on a card, on a small card of an account's page and on the dashboard.
 */
export function StashLines({ stash, locale, next = true }: { stash: Stash; /** The number and date format of the data (the settings'). */ locale: string; next?: boolean }) {
  const t = useT()
  const money = stashMoney(stash, locale)
  const line = nextLine(stash)
  const words = line ? nextText(line, money, t, locale) : null
  return (
    <>
      <p className={cn('stash-figures', stash.target === null && 'is-big', stash.saved.startsWith('-') && 'is-below')}>{figures(stash, money, t)}</p>
      {next && words ? <p className={cn('stash-next', words.warning && 'is-late')}>{words.text}</p> : null}
    </>
  )
}

/** A small card of a stash that links to its page: its name, its numbers and a bar. On an account's page and on the dashboard. */
export function StashMini({ stash, locale }: { stash: Stash; locale: string }) {
  const t = useT()
  const money = stashMoney(stash, locale)
  return (
    <Link className="stash-mini" to={`/app/stashes/${stash.id}`}>
      <span className="stash-mini-text">
        <span className="stash-mini-name" title={stash.name}>
          {stash.name}
        </span>
        <span className={cn('stash-mini-figures', stash.saved.startsWith('-') && 'is-below')}>{figures(stash, money, t)}</span>
      </span>
      <StashBar saved={stash.saved} target={stash.target} />
    </Link>
  )
}
