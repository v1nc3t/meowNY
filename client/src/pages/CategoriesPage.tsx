import { useEffect, useRef, useState, type FormEvent } from 'react'
import AddCategoryModal from '../components/AddCategoryModal'
import { visibleCategories, type Category, type Kind } from '../categoriesView'
import { api, errorMessage } from '../http'

function kindLabel(kind: Kind): string {
  if (kind === 'INCOME') return 'income'
  if (kind === 'EXPENSE') return 'expense'
  return 'all'
}

export default function CategoriesPage() {
  const [categories, setCategories] = useState<Category[]>([])
  const [kind, setKind] = useState<Kind>('ALL')
  const [typeOpen, setTypeOpen] = useState(false)
  const typeRef = useRef<HTMLDivElement>(null)
  const [query, setQuery] = useState('')
  const [message, setMessage] = useState('')
  const [adding, setAdding] = useState(false)
  const [loaded, setLoaded] = useState(false)

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
    const response = await api('/api/v1/categories')
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    setCategories((await response.json()) as Category[])
    setLoaded(true)
  }

  useEffect(() => {
    void load()
  }, [])

  const shown = visibleCategories(categories, kind, query)

  return (
    <section className="tx-page">
      <div className="tx-title">
        <h1>categories</h1>
        <div className="tx-actions">
          <button type="button" onClick={() => setAdding(true)}>
            add category
          </button>
        </div>
      </div>
      <div className="toolbar wide tx-bar">
        <label className="grow">
          search
          <input
            value={query}
            maxLength={50}
            autoComplete="off"
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <div className="pick fit type" ref={typeRef}>
          <span>type</span>
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
        {loaded && shown.length === 0 ? <p>no categories.</p> : null}
        <ul className="rows">
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
  const [renaming, setRenaming] = useState(false)

  async function onSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const name = String(form.get('name') ?? '').trim()
    if (!name) {
      onError('enter a category name.')
      return
    }
    const response = await api(`/api/v1/categories/${encodeURIComponent(category.id)}`, {
      method: 'PATCH',
      body: JSON.stringify({ name }),
    })
    if (!response.ok) {
      onError(await errorMessage(response))
      return
    }
    onError('')
    setRenaming(false)
    await onDone()
  }

  async function onDelete() {
    const response = await api(`/api/v1/categories/${encodeURIComponent(category.id)}`, { method: 'DELETE' })
    if (!response.ok) {
      onError(await errorMessage(response))
      return
    }
    onError('')
    await onDone()
  }

  const typeLabel = category.type === 'INCOME' ? 'income' : 'expense'

  if (renaming) {
    return (
      <li>
        <form className="entry-row" onSubmit={(event) => void onSave(event)}>
          <label>
            name
            <input name="name" required maxLength={50} defaultValue={category.name} autoComplete="off" />
          </label>
          <div className="entry-actions">
            <button type="submit">save</button>
            <button type="button" onClick={() => setRenaming(false)}>
              cancel
            </button>
          </div>
        </form>
      </li>
    )
  }

  return (
    <li>
      <div className="entry-row">
        <p className="entry-main">
          {category.name}
          <span className="entry-meta">{typeLabel}</span>
        </p>
        <div className="entry-actions">
          <button type="button" className="tx-text" onClick={() => setRenaming(true)}>
            rename
          </button>
          <button type="button" className="tx-text" onClick={() => void onDelete()}>
            delete
          </button>
        </div>
      </div>
    </li>
  )
}
