import { useI18n } from '../../i18n/context'
import { SPECS, sizeRules, WIDGET_TYPES, type Profile, type WidgetType } from '../../lib/dashboardWidgets'
import Modal from '../Modal'

const range = (least: number, most: number) => (least === most ? String(least) : `${least}-${most}`)

/** The sizes a widget can take, "3-6 x 1-2" in columns and rows. */
function sizes(type: WidgetType, profile: Profile): string {
  const { widths, minH, maxH } = sizeRules(type, profile)
  return `${range(widths[0], widths[widths.length - 1])} × ${range(minH, maxH)}`
}

/** Every widget there is, with its name, a line about it and the sizes it can take; choosing one adds it at the end of the dashboard. */
export default function CatalogDialog({ profile, onChoose, onClose }: { profile: Profile; onChoose: (type: WidgetType) => void; onClose: () => void }) {
  const { t } = useI18n()
  return (
    <Modal title={t('dashboard.addWidget')} onClose={onClose} cancelLabel={t('common.close')} readOnly wide>
      <ul className="dash-catalog">
        {WIDGET_TYPES.map((type) => (
          <li key={type}>
            <button type="button" onClick={() => onChoose(type)}>
              <span className="dash-catalog-name">{t(SPECS[type].name)}</span>
              <span className="dash-catalog-about">{t(SPECS[type].about)}</span>
              <span className="dash-catalog-sizes">{sizes(type, profile)}</span>
            </button>
          </li>
        ))}
      </ul>
    </Modal>
  )
}
