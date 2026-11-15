import { useState, type FormEvent } from 'react'
import { allChosen, choicesInOrder, parseEvery, type Choice, type Conflict } from '../api/sync'
import { useAccounts } from '../hooks/useAccounts'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { formatMoney } from '../lib/money'
import Modal, { modalFormId } from './Modal'
import Button from './ui/Button'

type Side = 'desktop' | 'phone'

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
    if (conflict.table === 'transfers') return accounts?.some((account) => account.foreign) ? undefined : settings?.currency
    if (!settings || conflict.table !== 'accounts') return settings?.currency
    const said = conflict.fields.find((field) => field.field === 'currency')
    if (said) return (choice === 'mine' ? said.mine : said.theirs) || settings.currency
    const account = accounts?.find((each) => each.name === conflict.title)
    return account?.foreign ? account.currency : settings.currency
  }

  function valueText(field: Conflict['fields'][number]['field'], value: string, currency: string | undefined): string {
    if (value === '') return '-'
    // what a transaction or a transfer was in its account's currency, and a rate, are numbers: the conflict does not say which currency (an account's amounts are written in its own)
    if ((field === 'amount' || field === 'openingBalance' || field === 'virtualBalance') && settings && currency) return formatMoney(value, currency, settings.locale)
    // the type of an account is one of its four kinds; of a category, income or expense
    if (field === 'type' && (value === 'CHECKING' || value === 'SAVINGS' || value === 'CASH' || value === 'CREDIT_CARD')) return t(`accounts.kind.${value}`)
    if (field === 'type') return t(value === 'INCOME' ? 'common.income' : 'common.expense')
    if (field === 'active' || field === 'keepOnPhone' || field === 'archived' || field === 'includeNetWorth') return t(value === 'yes' ? 'common.yes' : 'common.no')
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
                          <span>{t(`sync.field.${field.field}`)}</span>
                          <b>{valueText(field.field, choice === 'mine' ? field.mine : field.theirs, currencyOf(conflict, choice))}</b>
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
