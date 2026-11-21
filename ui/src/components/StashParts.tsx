import { Link } from 'react-router-dom'
import type { Stash } from '../api/stashes'
import { useT } from '../i18n/context'
import { formatDate } from '../lib/money'
import { rateWords } from '../lib/loansView'
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

/** The bar of a row of the list (`rowMeter`): a goal's progress to its target, or a deposit's way through its term. Only a picture: its figure is beside it. */
export function MeterBar({ fill, reached }: { fill: number; reached: boolean }) {
  return (
    <span className={cn('stash-bar', reached && 'is-reached')} aria-hidden="true">
      <span className="stash-fill" style={{ width: `${fill * 100}%` }} />
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

/**
 * What a term deposit says of itself, on its page (docs/loans-deposits-design.md, section 10), as the facts of a loan are laid out: the rate, the start and the
 * maturity, the days left and the return it expects, the value it will have, and where it stands (with the day it was booked or broken). The expected
 * return and the value are left out once it is booked or broken (they are worked out on what it holds, which is no longer the money of its term), the days
 * left once it has matured. The numbers are the app's (`Stash.deposit`).
 */
export function DepositFacts({ stash, locale }: { stash: Stash; locale: string }) {
  const t = useT()
  const deposit = stash.deposit
  if (!deposit) return null
  const money = stashMoney(stash, locale)
  const live = deposit.state === 'RUNNING' || deposit.state === 'MATURED'
  const day = deposit.brokenOn ?? (deposit.state === 'BOOKED' ? deposit.returnedOn : null)
  const rows: [string, string][] = [
    [t('accounts.rate'), rateWords(deposit, t)],
    [t('stashes.startDate'), formatDate(deposit.startDate, locale)],
    [t('stashes.maturity'), stash.targetDate === null ? '' : formatDate(stash.targetDate, locale)],
    [t('stashes.daysLeft'), deposit.state === 'RUNNING' ? String(deposit.daysLeft) : ''],
    [t('stashes.expectedReturn'), live ? money(deposit.expectedReturn) : ''],
    [t('stashes.valueAtMaturity'), live ? money(deposit.valueAtMaturity) : ''],
    [t('stashes.depositState'), day ? `${t(`stashes.state.${deposit.state}`)} · ${formatDate(day, locale)}` : t(`stashes.state.${deposit.state}`)],
  ]
  return (
    <div className="dash-rec-sum">
      {rows
        .filter(([, value]) => value !== '')
        .map(([label, value]) => (
          <span key={label}>
            <span className="dash-stat-label">{label}</span>
            <b>{value}</b>
          </span>
        ))}
    </div>
  )
}
