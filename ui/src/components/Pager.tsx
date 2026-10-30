import { useProfile } from '../hooks/profile'
import { useT } from '../i18n/context'
import { pageItems } from '../lib/pager'

/**
 * The pages of a list: previous, the first page, the pages around this one, the last page and next. A gap (…) stands for the pages
 * left out. A phone has room for fewer neighbours. Nothing is shown for a single page.
 */
export default function Pager({ page, count, onPage }: { page: number; count: number; onPage: (page: number) => void }) {
  const t = useT()
  const phone = useProfile() === 'phone'
  if (count < 2) return null
  return (
    <nav className="page-numbers" aria-label={t('pager.label')}>
      <button type="button" aria-label={t('pager.prev')} disabled={page <= 1} onClick={() => onPage(page - 1)}>
        &lsaquo;
      </button>
      {pageItems(page, count, phone ? 0 : 2).map((item, index) =>
        item === 'gap' ? (
          <span key={`gap-${index}`} aria-hidden="true">
            …
          </span>
        ) : (
          <button
            key={item}
            type="button"
            aria-label={t('pager.page', { page: item, pages: count })}
            aria-current={item === page ? 'page' : undefined}
            onClick={() => onPage(item)}
          >
            {item}
          </button>
        ),
      )}
      <button type="button" aria-label={t('pager.next')} disabled={page >= count} onClick={() => onPage(page + 1)}>
        &rsaquo;
      </button>
    </nav>
  )
}
