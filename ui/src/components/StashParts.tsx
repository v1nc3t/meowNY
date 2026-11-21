import { Link } from 'react-router-dom'
import type { Stash } from '../api/stashes'
import { useT } from '../i18n/context'
import { figures, progress, stashMoney } from '../lib/stashView'
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
