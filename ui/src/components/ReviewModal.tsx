import { useState, type FormEvent } from 'react'
import { ACCOUNT_KINDS, type AccountKind } from '../api/accounts'
import { allChosen, choicesInOrder, parseEvery, type Choice, type Conflict } from '../api/sync'
import { useAccounts } from '../hooks/useAccounts'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import type { MessageKey } from '../i18n/en'
import { formatMoney } from '../lib/money'
import Modal, { modalFormId } from './Modal'
import { Button } from './ui/button'

type Side = 'desktop' | 'phone'

/** The words of a field of a conflict. `target` is the "linked to" of a document link, and the target amount of a stash. */
function fieldWords(table: Conflict['table'], field: Conflict['fields'][number]['field']): MessageKey {
  return table === 'stashes' && field === 'target' ? 'sync.field.stashTarget' : `sync.field.${field}`
}

/**
 * The same things were changed on both devices since they last met: for each, the person says which version is true. This device
 * is the one that takes the changes in, so "mine" is the version here and "theirs" the one that is coming. Nothing is chosen for them.
 */
export default function ReviewModal({
  conflicts,
  more,
  side,
  onDecide,
  onGiveUp,
}: {
  conflicts: Conflict[]
  more: number
  side: Side
  onDecide: (choices: Choice[]) => void
  onGiveUp: () => void
}) {
  const t = useT()
  const { settings } = useMoneySettings()
  const accounts = useAccounts()
  const [chosen, setChosen] = useState<Record<string, Choice | undefined>>({})
  const [pending, setPending] = useState(false)
  const mineLabel = t(side === 'desktop' ? 'sync.side.thisComputer' : 'sync.side.thisPhone')
  const theirsLabel = t(side === 'desktop' ? 'sync.side.yourPhone' : 'sync.side.yourComputer')

  /**
   * The currency an amount of an account is in, for the two versions of a conflict: what that version says its currency is (nothing: the app's), else
   * the currency of the account of that name on this device. The app's own for anything but an account; a transfer's amount is in the currency of its
   * first account, which the conflict does not name, so with an account in another currency it is written plain.
   */
  function currencyOf(conflict: Conflict, choice: 'mine' | 'theirs'): string | undefined {
    // an entry of a stash is in its account's currency, which the conflict does not name (it names the stash): plain with an account in another currency
    if (conflict.table === 'transfers' || conflict.table === 'stash_entries') return accounts?.some((account) => account.foreign) ? undefined : settings?.currency
    // a stash is in its account's currency: the account of that name on this device, as that version says it
    if (conflict.table === 'stashes' && settings) {
      const said = conflict.fields.find((field) => field.field === 'account')
      const named = said ? accounts?.find((account) => account.name === (choice === 'mine' ? said.mine : said.theirs)) : undefined
      return named ? (named.foreign ? named.currency : settings.currency) : accounts?.some((account) => account.foreign) ? undefined : settings.currency
    }
    if (!settings || conflict.table !== 'accounts') return settings?.currency
    const said = conflict.fields.find((field) => field.field === 'currency')
    if (said) return (choice === 'mine' ? said.mine : said.theirs) || settings.currency
    const account = accounts?.find((each) => each.name === conflict.title)
    return account?.foreign ? account.currency : settings.currency
  }

  function valueText(table: Conflict['table'], field: Conflict['fields'][number]['field'], value: string, currency: string | undefined): string {
    if (value === '') return '-'
    // what a transaction or a transfer was in its account's currency, and a rate, are numbers: the conflict does not say which currency (an account's amounts are written in its own)
    // the same for a stash's target
    if ((field === 'amount' || field === 'openingBalance' || field === 'virtualBalance' || (field === 'target' && table === 'stashes')) && settings && currency) return formatMoney(value, currency, settings.locale)
    // the type of an account is one of its kinds; of a category, income or expense
    if (field === 'type' && (ACCOUNT_KINDS as readonly string[]).includes(value)) return t(`accounts.kind.${value as AccountKind}`)
    if (field === 'type') return t(value === 'INCOME' ? 'common.income' : 'common.expense')
    if (field === 'active' || field === 'keepOnPhone' || field === 'archived' || field === 'includeNetWorth') return t(value === 'yes' ? 'common.yes' : 'common.no')
    if (field === 'kind' && table === 'stash_entries') return value.toUpperCase() === 'TAKE' ? t('stashes.kind.TAKE') : t('stashes.kind.ADD')
    // a stash is a goal or a deposit; a rate is per month or per year
    if (field === 'kind' && table === 'stashes' && (value === 'GOAL' || value === 'DEPOSIT')) return t(`stashes.kind.${value}`)
    if (field === 'interestPeriod' && (value === 'MONTHLY' || value === 'YEARLY')) return t(`accounts.period.${value}`)
    if (field === 'kind' && (value === 'LENT' || value === 'BORROWED' || value === 'REPAID_TO_ME' || value === 'REPAID_BY_ME')) return t(`debts.kind.${value}`)
    if (field === 'every') {
      const every = parseEvery(value)
      if (every) return every.count === 1 ? t(`rec.every1.${every.unit as 'DAY' | 'WEEK' | 'MONTH' | 'YEAR'}`) : t(`rec.everyN.${every.unit as 'DAY' | 'WEEK' | 'MONTH' | 'YEAR'}`, { count: every.count })
    }
    return value
  }

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const choices = choicesInOrder(conflicts, chosen)
    if (!choices || pending) return
    setPending(true)
    onDecide(choices)
  }

  function chooseAll(choice: Choice) {
    setChosen(Object.fromEntries(conflicts.map((conflict) => [conflict.id, choice])))
  }

  return (
    <Modal title={t('sync.reviewTitle')} pending={pending || !allChosen(conflicts, chosen)} onClose={onGiveUp}>
      <form id={modalFormId} className="review" onSubmit={onSubmit}>
        <div className="review-all">
          <Button variant="outline" size="sm" onClick={() => chooseAll('mine')}>
            {t('sync.allOf', { device: mineLabel })}
          </Button>
          <Button variant="outline" size="sm" onClick={() => chooseAll('theirs')}>
            {t('sync.allOf', { device: theirsLabel })}
          </Button>
        </div>
        {conflicts.map((conflict) => (
          <fieldset className="conflict" key={conflict.id}>
            <legend>
              {t(`sync.table.${conflict.table}`)}
              {conflict.title ? `: ${conflict.title}` : ''}
            </legend>
            <div className="conflict-versions">
              {(['mine', 'theirs'] as const).map((choice) => {
                const deleted = choice === 'mine' ? conflict.mineDeleted : conflict.theirsDeleted
                return (
                  <button
                    key={choice}
                    type="button"
                    className="conflict-version"
                    aria-pressed={chosen[conflict.id] === choice}
                    onClick={() => setChosen((current) => ({ ...current, [conflict.id]: choice }))}
                  >
                    <span className="conflict-who">{choice === 'mine' ? mineLabel : theirsLabel}</span>
                    {deleted ? (
                      <span className="conflict-deleted">{t('sync.deletedVersion')}</span>
                    ) : conflict.fields.length === 0 ? (
                      <span className="conflict-field">{t('sync.sameContent')}</span>
                    ) : (
                      conflict.fields.map((field) => (
                        <span className="conflict-field" key={field.field}>
                          <span>{t(fieldWords(conflict.table, field.field))}</span>
                          <b>{valueText(conflict.table, field.field, choice === 'mine' ? field.mine : field.theirs, currencyOf(conflict, choice))}</b>
                        </span>
                      ))
                    )}
                  </button>
                )
              })}
            </div>
          </fieldset>
        ))}
        {more > 0 ? <p className="hint">{t('sync.moreSettled', { count: more })}</p> : null}
      </form>
    </Modal>
  )
}
