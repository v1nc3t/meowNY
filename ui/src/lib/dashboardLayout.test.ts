import assert from 'node:assert/strict'
import { test } from 'node:test'
import { addWidget, defaultLayout, halve, moveWidget, parseLayout, removeWidget, resizeWidget, setWidgetSettings, toPhoneSpan, type Layout } from './dashboardLayout.ts'
import { clampSize, sizeRules, SPECS, WIDGET_TYPES } from './dashboardWidgets.ts'

const widget = (over: object) => ({ id: 'a', type: 'summary', w: 12, h: 1, settings: {}, ...over })
const layoutOf = (...widgets: object[]) => JSON.stringify({ version: 1, widgets })

test('a layout that is not readable is the default', () => {
  for (const bad of [null, '', 'not json', '{', '[]', '{"version":2,"widgets":[]}', '{"version":1}', '{"version":1,"widgets":5}', '"text"']) {
    assert.deepEqual(parseLayout(bad, 'desktop'), defaultLayout('desktop'), String(bad))
  }
})

test('an empty layout, or one with nothing left, is the default', () => {
  assert.deepEqual(parseLayout(layoutOf(), 'phone'), defaultLayout('phone'))
  assert.deepEqual(parseLayout(layoutOf(widget({ type: 'gone' })), 'phone'), defaultLayout('phone'))
})

test('the defaults are today\'s dashboard, with sizes the widgets allow', () => {
  assert.deepEqual(
    defaultLayout('desktop').widgets.map((w) => `${w.type} ${w.w}x${w.h}`),
    ['summary 12x1', 'trend 12x2', 'networth 6x2', 'budgets 6x2', 'stashes 6x2', 'upcoming 6x2', 'debts 6x2'],
  )
  assert.deepEqual(
    defaultLayout('phone').widgets.map((w) => `${w.type} ${w.w}x${w.h}`),
    ['summary 2x1', 'trend 2x2', 'networth 2x2', 'budgets 2x2', 'upcoming 2x2', 'stashes 2x2'],
  )
})

test('an unknown type and a wrong value are dropped, the rest kept', () => {
  const layout = parseLayout(
    layoutOf(widget({ id: 'x', type: 'weather' }), widget({ id: 'y', w: 'wide' }), widget({ id: 'z', h: null }), widget({ id: '' }), 5, null, widget({ id: 'ok', type: 'networth', w: 6, h: 2 })),
    'desktop',
  )
  assert.deepEqual(layout.widgets.map((w) => w.id), ['ok'])
})

test('a size outside the range is clamped, and a width in between snaps to an allowed one', () => {
  const layout = parseLayout(layoutOf(widget({ id: 'a', w: 99, h: 9 }), widget({ id: 'b', type: 'networth', w: 1, h: 0 }), widget({ id: 'c', type: 'trend', w: 7.4, h: 2.6 })), 'desktop')
  assert.deepEqual(layout.widgets.map((w) => [w.w, w.h]), [[12, 1], [4, 1], [6, 3]])
  const phone = parseLayout(layoutOf(widget({ id: 'a', w: 12 }), widget({ id: 'b', type: 'networth', w: 0 })), 'phone')
  assert.deepEqual(phone.widgets.map((w) => [w.w, w.h]), [[2, 1], [1, 1]])
})

test('a repeated id is dropped, and no more than the most are kept', () => {
  assert.equal(parseLayout(layoutOf(widget({}), widget({ type: 'trend', w: 6, h: 2 })), 'desktop').widgets.length, 1)
  const many = Array.from({ length: 80 }, (_, i) => widget({ id: `w${i}` }))
  assert.equal(parseLayout(layoutOf(...many), 'desktop').widgets.length, 40)
})

test('settings: bad fields and settings the widget does not have are left out; names and amounts are never kept', () => {
  const layout = parseLayout(
    layoutOf(
      widget({
        type: 'trend',
        w: 12,
        h: 2,
        settings: { period: { kind: 'yesterday' }, accountIds: ['1', '1', '2', 'x'], categoryIds: ['3'], count: 5, title: '  My\u0000 trend  ', name: 'Rent', balance: '5.00', stashId: '9' },
      }),
    ),
    'desktop',
  )
  // the list with a wrong id is wrong as a whole; the title is cleaned; count and stashId are not trend settings
  assert.deepEqual(layout.widgets[0].settings, { categoryIds: ['3'], title: 'My  trend' })
  const custom = parseLayout(layoutOf(widget({ settings: { period: { kind: 'custom', from: '2026-01-01' } } }), widget({ id: 'b', settings: { period: { kind: 'custom', from: '2026-01-01', to: '2026-02-28', x: 1 } } })), 'desktop')
  assert.deepEqual(custom.widgets.map((w) => w.settings.period), [undefined, { kind: 'custom', from: '2026-01-01', to: '2026-02-28' }])
  const count = parseLayout(layoutOf(widget({ type: 'stashes', w: 6, h: 2, settings: { count: 7 } })), 'desktop')
  assert.deepEqual(count.widgets[0].settings, {})
})

test('moving: to a place, beyond the ends, an id that is not there', () => {
  const layout = defaultLayout('desktop')
  const ids = (l: Layout) => l.widgets.map((w) => w.id)
  assert.deepEqual(ids(moveWidget(layout, 'debts', 0)).slice(0, 2), ['debts', 'summary'])
  assert.equal(ids(moveWidget(layout, 'summary', 99)).at(-1), 'summary')
  assert.equal(ids(moveWidget(layout, 'summary', -5))[0], 'summary')
  assert.equal(moveWidget(layout, 'nothing', 2), layout)
  assert.equal(moveWidget(layout, 'trend', 1).widgets.length, layout.widgets.length)
})

test('resizing is clamped to the widget, adding appends at its start size, removing drops it', () => {
  const layout = defaultLayout('desktop')
  const big = resizeWidget(layout, 'networth', 12, 9, 'desktop').widgets.find((w) => w.id === 'networth')
  assert.deepEqual([big?.w, big?.h], [6, 2])
  const added = addWidget(layout, 'stashes', 'desktop', 'n1')
  assert.deepEqual(added.widgets.at(-1), { id: 'n1', type: 'stashes', w: 6, h: 2, settings: {} })
  assert.equal(addWidget(added, 'stashes', 'desktop', 'n1'), added)
  assert.equal(resizeWidget(layout, 'networth', 6, 2, 'desktop'), layout)
  assert.equal(resizeWidget(layout, 'nothing', 6, 2, 'desktop'), layout)
  assert.equal(moveWidget(layout, 'summary', 0), layout)
  assert.equal(removeWidget(layout, 'trend').widgets.length, 6)
  assert.equal(removeWidget(layout, 'nothing').widgets.length, 7)
})

test('settings are cleaned when they are set', () => {
  const layout = setWidgetSettings(defaultLayout('desktop'), 'budgets', { count: 10, accountIds: ['1'], title: '   ' })
  assert.deepEqual(layout.widgets.find((w) => w.id === 'budgets')?.settings, { count: 10 })
})

test('spans: the phone has one or two columns, six columns halve, rounded up', () => {
  assert.deepEqual([3, 4, 6, 8, 9, 12].map(toPhoneSpan), [1, 1, 2, 2, 2, 2])
  assert.deepEqual([3, 4, 6, 8, 9, 12].map(halve), [2, 2, 3, 4, 5, 6])
})

test('every widget has sizes that fit its own range and a start inside them, on both profiles', () => {
  for (const type of WIDGET_TYPES) {
    for (const profile of ['desktop', 'phone'] as const) {
      const rules = sizeRules(type, profile)
      assert.ok(rules.widths.length > 0, `${type} ${profile}`)
      assert.deepEqual(clampSize(type, profile, rules.start.w, rules.start.h), rules.start, `${type} ${profile}`)
    }
    assert.ok(SPECS[type].h[1] <= 4)
  }
})
