import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MAX_REPORT_MONTHS, monthEnd, monthsBetween, problemWith, reportMonths, selectionFor, type Choice } from './exportPlan.ts'

const all: Choice = { limitDates: false, from: '2026-01-01', to: '2026-12-31', allCategories: true, categoryIds: [] }

test('every date and every category means no limits', () => {
  for (const format of ['json', 'csv', 'xlsx', 'pdf'] as const) {
    assert.deepEqual(selectionFor(format, all), { from: null, to: null, categoryIds: null })
  }
})

test('chosen dates and categories are sent as they are', () => {
  const choice: Choice = { ...all, limitDates: true, from: '2026-03-10', to: '2026-05-20', allCategories: false, categoryIds: ['4', '7'] }
  assert.deepEqual(selectionFor('csv', choice), { from: '2026-03-10', to: '2026-05-20', categoryIds: ['4', '7'] })
  assert.deepEqual(selectionFor('json', choice), selectionFor('xlsx', choice))
})

test('a PDF covers whole months, so its dates are widened to them', () => {
  const choice: Choice = { ...all, limitDates: true, from: '2026-03-10', to: '2026-05-20' }
  assert.deepEqual(selectionFor('pdf', choice), { from: '2026-03-01', to: '2026-05-31', categoryIds: null })
  assert.equal(monthEnd('2026-02'), '2026-02-28')
  assert.equal(monthEnd('2024-02'), '2024-02-29', 'leap year')
  assert.equal(monthEnd('2026-12'), '2026-12-31')
})

test('the months of a report come from the dates, the data or the current month', () => {
  const span = { firstDate: '2026-02-11', lastDate: '2026-04-30' }
  assert.deepEqual(reportMonths({ ...all, limitDates: true, from: '2026-06-15', to: '2026-08-02' }, span, '2026-10'), {
    fromMonth: '2026-06',
    toMonth: '2026-08',
  })
  assert.deepEqual(reportMonths(all, span, '2026-10'), { fromMonth: '2026-02', toMonth: '2026-04' })
  assert.deepEqual(reportMonths(all, { firstDate: null, lastDate: null }, '2026-10'), { fromMonth: '2026-10', toMonth: '2026-10' })
})

test('months are counted from the first to the last, both included', () => {
  assert.equal(monthsBetween('2026-03', '2026-03'), 1)
  assert.equal(monthsBetween('2025-11', '2026-02'), 4)
  assert.equal(monthsBetween('2016-01', '2025-12'), MAX_REPORT_MONTHS)
  assert.equal(monthsBetween('2016-01', '2026-01'), MAX_REPORT_MONTHS + 1)
})

test('a choice with an end before its start, no categories or too long a report is refused', () => {
  assert.equal(problemWith('csv', all, 1), null)
  assert.equal(problemWith('csv', { ...all, limitDates: true, from: '2026-05-01', to: '2026-04-30' }, 1), 'dates')
  assert.equal(problemWith('csv', { ...all, limitDates: true, from: '2026-05-01', to: '2026-05-01' }, 1), null, 'one day is fine')
  assert.equal(problemWith('json', { ...all, allCategories: false, categoryIds: [] }, 1), 'categories')
  assert.equal(problemWith('json', { ...all, allCategories: false, categoryIds: ['3'] }, 1), null)
  assert.equal(problemWith('pdf', all, MAX_REPORT_MONTHS + 1), 'months')
  assert.equal(problemWith('xlsx', all, MAX_REPORT_MONTHS + 1), null, 'only a report has a length limit')
})
