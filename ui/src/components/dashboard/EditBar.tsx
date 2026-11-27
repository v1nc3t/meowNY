import { useT } from '../../i18n/context'
import HelpButton from '../HelpButton'
import Icon from '../NavIcon'
import { Button } from '../ui/button'

/** The slim bar under the header while the dashboard is being arranged: add a widget, put the default back, or finish. */
export default function EditBar({ onAdd, onReset, onDone }: { onAdd: () => void; onReset: () => void; onDone: () => void }) {
  const t = useT()
  return (
    <div className="dash-editbar" role="toolbar" aria-label={t('dashboard.customize')}>
      <HelpButton title={t('dashboard.customize')}>
        <p>{t('dashboard.editHelp')}</p>
        <p>{t('dashboard.editKeys')}</p>
      </HelpButton>
      <span className="dash-editbar-gap" />
      <Button variant="outline" size="sm" onClick={onAdd}>
        <Icon name="add" className="btn-icon" />
        {t('dashboard.addWidget')}
      </Button>
      <Button variant="outline" size="sm" onClick={onReset}>
        {t('dashboard.reset')}
      </Button>
      <Button size="sm" onClick={onDone}>
        {t('common.done')}
      </Button>
    </div>
  )
}
