import { useEffect, useState, type FormEvent } from 'react'
import Modal, { modalFormId } from '../components/Modal'
import { api, errorMessage } from '../http'

type Group = { id: string; name: string }
type Category = { id: string; name: string; type: string; categoryGroupId: string | null }
type Kind = 'ALL' | 'INCOME' | 'EXPENSE'
type Dialog = 'group' | 'category' | null

export default function CategoriesPage() {
  const [groups, setGroups] = useState<Group[]>([])
  const [categories, setCategories] = useState<Category[]>([])
  const [kind, setKind] = useState<Kind>('ALL')
  const [message, setMessage] = useState('')
  const [dialog, setDialog] = useState<Dialog>(null)
  const [dialogMessage, setDialogMessage] = useState('')
  const [renamingGroup, setRenamingGroup] = useState<string | null>(null)

  async function load() {
    const [groupResponse, categoryResponse] = await Promise.all([
      api('/api/v1/category-groups'),
      api('/api/v1/categories'),
    ])
    if (!groupResponse.ok) {
      setMessage(await errorMessage(groupResponse))
      return
    }
    if (!categoryResponse.ok) {
      setMessage(await errorMessage(categoryResponse))
      return
    }
    setGroups((await groupResponse.json()) as Group[])
    setCategories((await categoryResponse.json()) as Category[])
  }

  useEffect(() => {
    void load()
  }, [])

  function openDialog(next: Exclude<Dialog, null>) {
    setDialogMessage('')
    setDialog(next)
  }

  async function onCreateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const name = String(new FormData(form).get('name') ?? '').trim()
    if (!name) {
      setDialogMessage('enter a group name.')
      return
    }
    const response = await api('/api/v1/category-groups', {
      method: 'POST',
      body: JSON.stringify({ name }),
    })
    if (!response.ok) {
      setDialogMessage(await errorMessage(response))
      return
    }
    setDialog(null)
    setDialogMessage('')
    setMessage('')
    await load()
  }

  async function onCreateCategory(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const name = String(data.get('name') ?? '').trim()
    const type = String(data.get('type') ?? '')
    const groupId = String(data.get('categoryGroupId') ?? '')
    if (!name || (type !== 'INCOME' && type !== 'EXPENSE')) {
      setDialogMessage('enter a category name and type.')
      return
    }
    const response = await api('/api/v1/categories', {
      method: 'POST',
      body: JSON.stringify({
        name,
        type,
        ...(groupId ? { categoryGroupId: groupId } : {}),
      }),
    })
    if (!response.ok) {
      setDialogMessage(await errorMessage(response))
      return
    }
    setDialog(null)
    setDialogMessage('')
    setMessage('')
    await load()
  }

  async function onRenameGroup(event: FormEvent<HTMLFormElement>, id: string) {
    event.preventDefault()
    const name = String(new FormData(event.currentTarget).get('name') ?? '').trim()
    if (!name) {
      setMessage('enter a group name.')
      return
    }
    const response = await api(`/api/v1/category-groups/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify({ name }),
    })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    setRenamingGroup(null)
    setMessage('')
    await load()
  }

  async function onDeleteGroup(id: string) {
    const response = await api(`/api/v1/category-groups/${encodeURIComponent(id)}`, { method: 'DELETE' })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    setMessage('')
    await load()
  }

  const visible = categories.filter((category) => kind === 'ALL' || category.type === kind)
  const ungrouped = visible.filter((category) => category.categoryGroupId === null)

  return (
    <section>
      <h1>categories</h1>
      <div className="page-actions">
        <button type="button" onClick={() => openDialog('group')}>
          add group
        </button>
        <button type="button" onClick={() => openDialog('category')}>
          add category
        </button>
      </div>
      <p className="notice">{message}</p>
      {dialog === 'group' ? (
        <Modal title="add group" onClose={() => setDialog(null)}>
          <p className="notice">{dialogMessage}</p>
          <form id={modalFormId} onSubmit={(event) => void onCreateGroup(event)}>
            <label>
              name
              <input name="name" required maxLength={50} autoComplete="off" />
            </label>
          </form>
        </Modal>
      ) : null}
      {dialog === 'category' ? (
        <Modal title="add category" onClose={() => setDialog(null)}>
          <p className="notice">{dialogMessage}</p>
          <form id={modalFormId} onSubmit={(event) => void onCreateCategory(event)}>
            <label>
              name
              <input name="name" required maxLength={50} autoComplete="off" />
            </label>
            <label>
              type
              <select name="type" required defaultValue="EXPENSE">
                <option value="EXPENSE">expense</option>
                <option value="INCOME">income</option>
              </select>
            </label>
            <label>
              group
              <select name="categoryGroupId" defaultValue="">
                <option value="">no group</option>
                {groups.map((group) => (
                  <option key={group.id} value={group.id}>
                    {group.name}
                  </option>
                ))}
              </select>
            </label>
          </form>
        </Modal>
      ) : null}
      <label>
        show
        <select value={kind} onChange={(event) => setKind(event.target.value as Kind)}>
          <option value="ALL">all</option>
          <option value="INCOME">income</option>
          <option value="EXPENSE">expense</option>
        </select>
      </label>
      <h2>groups</h2>
      <ul className="rows">
        {groups.map((group) => (
          <li key={group.id}>
            {renamingGroup === group.id ? (
              <form className="entry-row" onSubmit={(event) => void onRenameGroup(event, group.id)}>
                <label>
                  group name
                  <input name="name" required maxLength={50} defaultValue={group.name} autoComplete="off" />
                </label>
                <div className="entry-actions">
                  <button type="submit">save</button>
                  <button type="button" onClick={() => setRenamingGroup(null)}>
                    cancel
                  </button>
                </div>
              </form>
            ) : (
              <div className="entry-row">
                <p className="entry-main">{group.name}</p>
                <div className="entry-actions">
                  <button type="button" onClick={() => setRenamingGroup(group.id)}>
                    rename
                  </button>
                  <button type="button" onClick={() => void onDeleteGroup(group.id)}>
                    delete
                  </button>
                </div>
              </div>
            )}
            <ul className="rows">
              {visible
                .filter((category) => category.categoryGroupId === group.id)
                .map((category) => (
                  <CategoryEditor
                    key={`${category.id}:${category.name}:${category.categoryGroupId ?? ''}`}
                    category={category}
                    groups={groups}
                    onDone={load}
                    onError={setMessage}
                  />
                ))}
            </ul>
          </li>
        ))}
      </ul>
      <h2>no group</h2>
      <ul className="rows">
        {ungrouped.map((category) => (
          <CategoryEditor
            key={`${category.id}:${category.name}:${category.categoryGroupId ?? ''}`}
            category={category}
            groups={groups}
            onDone={load}
            onError={setMessage}
          />
        ))}
      </ul>
    </section>
  )
}

function CategoryEditor({
  category,
  groups,
  onDone,
  onError,
}: {
  category: Category
  groups: Group[]
  onDone: () => Promise<void>
  onError: (message: string) => void
}) {
  const [renaming, setRenaming] = useState(false)

  async function onSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const name = String(form.get('name') ?? '').trim()
    const groupId = String(form.get('categoryGroupId') ?? '')
    if (!name) {
      onError('enter a category name.')
      return
    }
    const response = await api(`/api/v1/categories/${encodeURIComponent(category.id)}`, {
      method: 'PATCH',
      body: JSON.stringify({ name, categoryGroupId: groupId === '' ? null : groupId }),
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
          <label>
            group
            <select name="categoryGroupId" defaultValue={category.categoryGroupId ?? ''}>
              <option value="">no group</option>
              {groups.map((group) => (
                <option key={group.id} value={group.id}>
                  {group.name}
                </option>
              ))}
            </select>
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
          <button type="button" onClick={() => setRenaming(true)}>
            rename
          </button>
          <button type="button" onClick={() => void onDelete()}>
            delete
          </button>
        </div>
      </div>
    </li>
  )
}
