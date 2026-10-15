import { useEffect, useRef, useState, type FormEvent } from 'react'
import AddCategoryModal from '../components/AddCategoryModal'
import { visibleCategories, type Category, type Kind } from '../lib/categoriesView'
import { categories as categoriesApi, cleanCategoryName } from '../api/categories'
import { errorText } from '../api/invoke'
import { useNotice } from '../hooks/useNotice'
import { useT } from '../i18n/context'
import { useShortcut } from '../keys/registry'
import { tip } from '../keys/tip'
import { useRowNavigation } from '../keys/useRowNavigation'

export default function CategoriesPage() {
  const t = useT()
  const kindLabel = (kind: Kind) => (kind === 'INCOME' ? t('common.income') : kind === 'EXPENSE' ? t('common.expense') : t('categories.all'))
  const [categories, setCategories] = useState<Category[]>([])
  const [kind, setKind] = useState<Kind>('ALL')
  const [typeOpen, setTypeOpen] = useState(false)
  const typeRef = useRef<HTMLDivElement>(null)
  const [query, setQuery] = useState('')
  const [message, setMessage] = useNotice()
  const [adding, setAdding] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!typeOpen) return
    function onPointerDown(event: PointerEvent) {
      if (!typeRef.current?.contains(event.target as Node)) setTypeOpen(false)
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') setTypeOpen(false)
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [typeOpen])

  async function load() {
    try {
      setCategories(await categoriesApi.list())
      setLoaded(true)
    } catch (error) {
      setMessage(errorText(error))
    }
  }

  useEffect(() => {
    void load()
  }, [])

  const shown = visibleCategories(categories, kind, query)

  // Keyboard: n adds, / searches, j and k move between rows.
  useShortcut('new', () => setAdding(true))
  useShortcut('search', () => searchRef.current?.focus())
  useRowNavigation()

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>{t('categories.title')}</h1>
        <div className="tx-actions">
          <button type="button" onClick={() => setAdding(true)} {...tip('new', t('categories.add'))}>
            {t('categories.add')}
          </button>
        </div>
      </div>
      <div className="toolbar wide tx-bar">
        <label className="grow" {...tip('search')}>
          {t('common.search')}
          <input
            ref={searchRef}
            onKeyDown={(event) => {
              if (event.key === 'Escape') event.currentTarget.blur()
            }}
            value={query}
            maxLength={50}
            autoComplete="off"
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <div className="pick fit type" ref={typeRef}>
          <span>{t('common.type')}</span>
          <button
            type="button"
            className="pick-button"
            aria-expanded={typeOpen}
            aria-haspopup="listbox"
            onClick={() => setTypeOpen((open) => !open)}
          >
            {kindLabel(kind)}
          </button>
          {typeOpen ? (
            <ul className="pick-list" role="listbox">
              {(['ALL', 'INCOME', 'EXPENSE'] as const).map((value) => (
                <li key={value}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={kind === value}
                    onClick={() => {
                      setKind(value)
                      setTypeOpen(false)
                    }}
                  >
                    {kindLabel(value)}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </div>
      <p className="notice">{message}</p>
      {adding ? (
        <AddCategoryModal
          onClose={() => setAdding(false)}
          onCreated={() => {
            setAdding(false)
            setMessage('')
            void load()
          }}
        />
      ) : null}
      <div className="tx-pane wide">
        {loaded && shown.length === 0 ? <p>{t('common.noCategories')}</p> : null}
        <ul className="card-list">
          {shown.map((category) => (
            <CategoryEditor
              key={`${category.id}:${category.name}`}
              category={category}
              onDone={load}
              onError={setMessage}
            />
          ))}
        </ul>
      </div>
    </section>
  )
}

function CategoryEditor({
  category,
  onDone,
  onError,
}: {
  category: Category
  onDone: () => Promise<void>
  onError: (message: string) => void
}) {
  const t = useT()
  const [renaming, setRenaming] = useState(false)

  async function onSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const name = cleanCategoryName(String(form.get('name') ?? ''))
    if (!name) {
      onError(t('categories.nameError'))
      return
    }
    try {
      await categoriesApi.rename(category.id, name)
    } catch (error) {
      onError(errorText(error))
      return
    }
    onError('')
    setRenaming(false)
    await onDone()
  }

  async function onDelete() {
    try {
      await categoriesApi.remove(category.id)
    } catch (error) {
      onError(errorText(error))
      return
    }
    onError('')
    await onDone()
  }

  const typeLabel = category.type === 'INCOME' ? t('common.income') : t('common.expense')

  if (renaming) {
    return (
      <li className="card-item">
        <form className="card-form" onSubmit={(event) => void onSave(event)}>
          <label>
            {t('common.name')}
            <input name="name" required maxLength={50} defaultValue={category.name} autoComplete="off" />
          </label>
          <div className="card-actions">
            <button type="submit">{t('common.save')}</button>
            <button type="button" onClick={() => setRenaming(false)}>
              {t('common.cancel')}
            </button>
          </div>
        </form>
      </li>
    )
  }

  return (
    <li className="card-item">
      <div className="card-body">
        <p className="card-name">{category.name}</p>
        <p className="card-meta">{typeLabel}</p>
      </div>
      <div className="card-actions">
        <button type="button" data-row-focus onClick={() => setRenaming(true)}>
          {t('common.rename')}
        </button>
        <button type="button" onClick={() => void onDelete()}>
          {t('common.delete')}
        </button>
      </div>
    </li>
  )
}
