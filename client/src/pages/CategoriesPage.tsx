import { useEffect, useState, type FormEvent } from 'react'
import { api, errorMessage } from '../http'

type Group = { id: string; name: string }
type Category = { id: string; name: string; type: string; categoryGroupId: string | null }
type Kind = 'ALL' | 'INCOME' | 'EXPENSE'

export default function CategoriesPage() {
  const [groups, setGroups] = useState<Group[]>([])
  const [categories, setCategories] = useState<Category[]>([])
  const [kind, setKind] = useState<Kind>('ALL')
  const [message, setMessage] = useState('')

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

  async function onCreateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const name = String(new FormData(form).get('name') ?? '').trim()
    if (!name) {
      setMessage('Enter a group name.')
      return
    }
    const response = await api('/api/v1/category-groups', {
      method: 'POST',
      body: JSON.stringify({ name }),
    })
    if (!response.ok) {
      setMessage(await errorMessage(response))
      return
    }
    form.reset()
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
      setMessage('Enter a category name and type.')
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
      setMessage(await errorMessage(response))
      return
    }
    form.reset()
    setMessage('')
    await load()
  }

  async function onRenameGroup(event: FormEvent<HTMLFormElement>, id: string) {
    event.preventDefault()
    const name = String(new FormData(event.currentTarget).get('name') ?? '').trim()
    if (!name) {
      setMessage('Enter a group name.')
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
      <h1>Categories</h1>
      <p>{message}</p>
      <form onSubmit={(event) => void onCreateGroup(event)}>
        <h2>New group</h2>
        <label>
          Name
          <input name="name" required maxLength={50} autoComplete="off" />
        </label>
        <button type="submit">Create group</button>
      </form>
      <form onSubmit={(event) => void onCreateCategory(event)}>
        <h2>New category</h2>
        <label>
          Name
          <input name="name" required maxLength={50} autoComplete="off" />
        </label>
        <label>
          Type
          <select name="type" required defaultValue="EXPENSE">
            <option value="EXPENSE">Expense</option>
            <option value="INCOME">Income</option>
          </select>
        </label>
        <label>
          Group
          <select name="categoryGroupId" defaultValue="">
            <option value="">No group</option>
            {groups.map((group) => (
              <option key={group.id} value={group.id}>
                {group.name}
              </option>
            ))}
          </select>
        </label>
        <button type="submit">Create category</button>
      </form>
      <label>
        Show
        <select value={kind} onChange={(event) => setKind(event.target.value as Kind)}>
          <option value="ALL">All</option>
          <option value="INCOME">Income</option>
          <option value="EXPENSE">Expense</option>
        </select>
      </label>
      <h2>Groups</h2>
      <ul>
        {groups.map((group) => (
          <li key={group.id}>
            <form onSubmit={(event) => void onRenameGroup(event, group.id)}>
              <label>
                Group name
                <input name="name" required maxLength={50} defaultValue={group.name} key={group.name} autoComplete="off" />
              </label>
              <button type="submit">Rename {group.name}</button>
            </form>
            <button type="button" onClick={() => void onDeleteGroup(group.id)}>
              Delete {group.name}
            </button>
            <ul>
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
      <h2>No group</h2>
      <ul>
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
  async function onSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = new FormData(event.currentTarget)
    const name = String(form.get('name') ?? '').trim()
    const groupId = String(form.get('categoryGroupId') ?? '')
    if (!name) {
      onError('Enter a category name.')
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

  const typeLabel = category.type === 'INCOME' ? 'Income' : 'Expense'

  return (
    <li>
      <p>{typeLabel}</p>
      <form onSubmit={(event) => void onSave(event)}>
        <label>
          Name
          <input name="name" required maxLength={50} defaultValue={category.name} autoComplete="off" />
        </label>
        <label>
          Group
          <select name="categoryGroupId" defaultValue={category.categoryGroupId ?? ''}>
            <option value="">No group</option>
            {groups.map((group) => (
              <option key={group.id} value={group.id}>
                {group.name}
              </option>
            ))}
          </select>
        </label>
        <button type="submit">Save {category.name}</button>
      </form>
      <button type="button" onClick={() => void onDelete()}>
        Delete {category.name}
      </button>
    </li>
  )
}
