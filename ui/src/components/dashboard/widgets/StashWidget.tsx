import { useT } from '../../../i18n/context'
import { formatDate } from '../../../lib/money'
import { activeStashes, stashMoney } from '../../../lib/stashView'
import { formatPercent } from '../../../lib/money'
import { useWidget } from '../context'
import { loadStashes, useData } from '../data'
import Bar from '../Bar'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** One stash: what is saved, a bar to its target with the percent, and under it the target and the day it is for. */
export default function StashWidget() {
  const t = useT()
  const { widget, money } = useWidget()
  const stashes = useData('stashes', loadStashes)
  if (!stashes) return <WidgetFrame />
  const active = activeStashes(stashes.list)
  const stash = active.find((each) => each.id === widget.settings.stashId) ?? active[0]
  if (!stash) {
    return (
      <WidgetFrame to="/app/stashes">
        <Empty text={t('dashboard.noStashes')} />
      </WidgetFrame>
    )
  }
  const fmt = stashMoney(stash, money.locale)
  return (
    <WidgetFrame to={`/app/stashes/${stash.id}`} name={stash.name}>
      <div className="dash-stat">
        <span className="dash-stat-label is-optional">{t('stashes.savedLabel')}</span>
        <span className="dash-stat-value">{fmt(stash.saved)}</span>
      </div>
      {stash.target === null || stash.percent === null ? null : (
        <>
          <Bar percent={stash.percent} label={formatPercent(stash.percent, money.locale)} />
          <span className="dash-foot">
            <span className="legend-amount" title={t('stashes.target')}>
              {fmt(stash.target)}
            </span>
            <span className="dash-foot-day" title={t('stashes.targetDate')}>
              {stash.targetDate ? formatDate(stash.targetDate, money.locale) : ''}
            </span>
          </span>
        </>
      )}
    </WidgetFrame>
  )
}
