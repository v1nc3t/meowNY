import { useState, type FormEvent } from 'react'
import { allChosen, choicesInOrder, parseEvery, type Choice, type Conflict } from '../api/sync'
import { useMoneySettings } from '../hooks/useMoneySettings'
import { useT } from '../i18n/context'
import { formatMoney } from '../lib/money'
import Modal, { modalFormId } from './Modal'

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
  const [chosen, setChosen] = useState<Record<string, Choice | undefined>>({})
  const [pending, setPending] = useState(false)
  const mineLabel = t(side === 'desktop' ? 'sync.side.thisComputer' : 'sync.side.thisPhone')
  const theirsLabel = t(side === 'desktop' ? 'sync.side.yourPhone' : 'sync.side.yourComputer')

  function valueText(field: Conflict['fields'][number]['field'], value: string): string {
    if (value === '') return '-'
    if (field === 'amount' && settings) return formatMoney(value, settings.currency, settings.locale)
    if (field === 'type') return t(value === 'INCOME' ? 'common.income' : 'common.expense')
    if (field === 'active' || field === 'keepOnPhone') return t(value === 'yes' ? 'common.yes' : 'common.no')
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
          <button type="button" onClick={() => chooseAll('mine')}>
            {t('sync.allOf', { device: mineLabel })}
          </button>
          <button type="button" onClick={() => chooseAll('theirs')}>
            {t('sync.allOf', { device: theirsLabel })}
          </button>
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
                          <b>{valueText(field.field, choice === 'mine' ? field.mine : field.theirs)}</b>
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
