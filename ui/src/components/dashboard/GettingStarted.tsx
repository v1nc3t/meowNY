import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { accounts } from '../../api/accounts'
import { budgets } from '../../api/budgets'
import { categories } from '../../api/categories'
import { debts } from '../../api/debts'
import { documents } from '../../api/documents'
import { recurring } from '../../api/recurring'
import { isServed } from '../../api/served'
import { stashes } from '../../api/stashes'
import { syncApi } from '../../api/sync'
import { transactions } from '../../api/transactions'
import { vault } from '../../vault/api'
import { useProfile } from '../../hooks/profile'
import { useT } from '../../i18n/context'
import { accountsSetUp, nextStep, stepsFor, type StepId } from '../../lib/gettingStarted'
import NavIcon from '../NavIcon'
import { Button } from '../ui/button'

/** Asks the app, for each step shown, whether it is done. A question that fails counts as not done. */
const CHECKS: Record<StepId, () => Promise<boolean>> = {
  vault: async () => true,
  accounts: async () => accountsSetUp(await accounts.list()),
  categories: async () => (await categories.list()).length > 0,
  transaction: async () => (await transactions.list({ limit: 1 })).total > 0,
  budget: async () => (await budgets.history()).length > 0,
  recurring: async () => (await recurring.list()).length > 0,
  stash: async () => (await stashes.list()).length > 0,
  friend: async () => (await debts.people(true)).length + (await debts.people(false)).length > 0,
  document: async () => (await documents.list({ limit: 1 })).total > 0,
  backup: async () => (await vault.backupSchedule()) !== null,
  phone: async () => (await syncApi.devices()).length > 0,
}

/**
 * "Getting started" at the top of the dashboard: every part of the app as a step, from creating the data to pairing a phone, each a link
 * to the page that does it and ticked off by what the app holds (looked at each time the dashboard opens). A bar shows how far along the
 * person is. It goes when they hide it; "show getting started" in the dashboard's "more" menu brings it back.
 */
export default function GettingStarted({ onHide }: { onHide: () => void }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  const [steps] = useState(() => stepsFor({ phone, served: isServed() }))
  const [done, setDone] = useState<ReadonlySet<StepId> | null>(null)

  useEffect(() => {
    let cancelled = false
    void Promise.allSettled(steps.map((step) => CHECKS[step.id]())).then((answers) => {
      if (cancelled) return
      setDone(new Set(steps.filter((_, at) => answers[at].status === 'fulfilled' && answers[at].value).map((step) => step.id)))
    })
    return () => {
      cancelled = true
    }
  }, [steps])

  if (!done) return null
  const count = done.size
  const next = nextStep(steps, done)
  const all = next === null

  return (
    <section className="guide" aria-labelledby="guide-title">
      <div className="guide-head">
        <h2 id="guide-title">{t('guide.title')}</h2>
        <span className="guide-count">{t('guide.count', { done: count, count: steps.length })}</span>
        <Button variant="ghost" size="icon" className="icon-btn" aria-label={t('guide.hide')} title={t('guide.hide')} onClick={onHide}>
          <NavIcon name="x" className="btn-icon" />
        </Button>
      </div>
      <div
        className="guide-bar"
        role="progressbar"
        aria-label={t('guide.title')}
        aria-valuemin={0}
        aria-valuemax={steps.length}
        aria-valuenow={count}
        aria-valuetext={t('guide.count', { done: count, count: steps.length })}
      >
        {steps.map((step) => (
          <span key={step.id} className={done.has(step.id) ? 'is-done' : undefined} />
        ))}
      </div>
      {all ? (
        <div className="guide-end">
          <p>{t('guide.allDone')}</p>
          <Button variant="outline" onClick={onHide}>
            {t('common.close')}
          </Button>
        </div>
      ) : (
        <ol className="guide-list">
          {steps.map((step) => {
            const finished = done.has(step.id)
            const body = (
              <>
                <span className="guide-mark" aria-hidden="true">
                  <NavIcon name={finished ? 'check' : step.icon} className="btn-icon" />
                </span>
                <span className="guide-text">
                  <b>{t(step.title)}</b>
                  <span>{t(step.text)}</span>
                </span>
              </>
            )
            return (
              <li key={step.id} className={finished ? 'is-done' : step.id === next ? 'is-next' : undefined}>
                {finished ? (
                  <span className="guide-step">
                    {body}
                    <span className="sr-only">{t('guide.doneMark')}</span>
                  </span>
                ) : (
                  <Link className="guide-step" to={step.to} state={step.state}>
                    {body}
                  </Link>
                )}
              </li>
            )
          })}
        </ol>
      )}
    </section>
  )
}
