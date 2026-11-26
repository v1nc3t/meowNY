import { useT } from '../../../i18n/context'
import { formatMoney } from '../../../lib/money'
import { activeStashes } from '../../../lib/stashView'
import Facts from '../../Facts'
import { StashMini } from '../../StashParts'
import { SPECS } from '../../../lib/dashboardWidgets'
import { useWidget } from '../context'
import { loadStashes, useData } from '../data'
import WidgetFrame, { Empty } from '../WidgetFrame'

/** The total set aside, how many stashes reached their target, and the first few stashes with a bar each. */
export default function StashesWidget() {
  const t = useT()
  const { widget, money } = useWidget()
  const stashes = useData('stashes', loadStashes)
  const count = widget.settings.count ?? SPECS.stashes.settings.count?.start ?? 3
  return (
    <WidgetFrame to="/app/stashes" className="dash-stashes">
      {stashes ? (
        stashes.summary.stashes === 0 ? (
          <Empty text={t('dashboard.noStashes')} />
        ) : (
          <>
            <Facts
              className="dash-facts"
              items={[
                { label: t('stashes.total'), value: <b>{formatMoney(stashes.summary.saved, money.currency, money.locale)}</b> },
                { label: t('stashes.reached'), value: `${stashes.summary.reached} / ${stashes.summary.stashes}` },
              ]}
            />
            {activeStashes(stashes.list)
              .slice(0, count)
              .map((stash) => (
                <StashMini key={stash.id} stash={stash} locale={money.locale} />
              ))}
          </>
        )
      ) : null}
    </WidgetFrame>
  )
}
