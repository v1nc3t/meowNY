import assert from 'node:assert/strict'
import { test } from 'node:test'
import { EXPORT_SECTIONS } from '../api/dataExport.ts'
import { amountText, bounds, defaultChoice, isLimited, fitFormat, MAX_REPORT_MONTHS, modesFor, monthEnd, problemWith, sectionsFor, selectionFor, withMode, type Choice } from './exportPlan.ts'

const all: Choice = { ...defaultChoice({ date: '2026-10-16', month: '2026-10' }), mode: 'date', from: '2026-01-01', to: '2026-12-31' }
const days = (from: string, to: string): Choice => ({ ...all, limitDates: true, mode: 'date', from, to })
const months = (from: string, to: string): Choice => ({ ...all, limitDates: true, mode: 'month', from, to })
const years = (from: string, to: string): Choice => ({ ...all, limitDates: true, mode: 'year', from, to })

test('a range by day, by month and by year is sent as the days it covers, the same for every format but a report', () => {
  const picks: [Choice, { from: string; to: string }][] = [
    [days('2026-03-10', '2026-05-20'), { from: '2026-03-10', to: '2026-05-20' }],
    [months('2026-03', '2026-05'), { from: '2026-03-01', to: '2026-05-31' }],
    [years('2025', '2026'), { from: '2025-01-01', to: '2026-12-31' }],
  ]
  for (const [choice, expected] of picks) {
    for (const format of ['json', 'csv', 'xlsx'] as const) {
      // with a range the history of debts, a part of a JSON or Excel file, has to be asked for
      const asked = format === 'csv' ? {} : { includeDebts: true }
      assert.deepEqual(selectionFor(format, choice), { ...expected, categoryIds: null, ...asked }, `${format} ${choice.mode}`)
    }
    assert.deepEqual(bounds(choice), expected)
  }
})

test('a report is made of whole months, so it has no choice by day and a range by day becomes its months', () => {
  assert.deepEqual(modesFor('pdf'), ['month', 'year'])
  for (const format of ['json', 'csv', 'xlsx'] as const) assert.deepEqual(modesFor(format), ['date', 'month', 'year'])
  const fitted = fitFormat('pdf', days('2026-03-10', '2026-05-20'))
  assert.deepEqual([fitted.mode, fitted.from, fitted.to], ['month', '2026-03', '2026-05'])
  // what already fits is left alone, for a report and for the others
  assert.equal(fitFormat('pdf', months('2026-03', '2026-05')).mode, 'month')
  assert.equal(fitFormat('pdf', years('2025', '2026')).mode, 'year')
  const unchanged = days('2026-03-10', '2026-05-20')
  assert.equal(fitFormat('csv', unchanged), unchanged)
  // a report asked for by day despite that (it cannot be, on the screen) is still widened to the months, never cut short
  assert.deepEqual(selectionFor('pdf', days('2026-03-10', '2026-05-20')), { from: '2026-03-01', to: '2026-05-31', categoryIds: null })
  assert.equal(monthEnd('2026-02'), '2026-02-28')
  assert.equal(monthEnd('2024-02'), '2024-02-29', 'leap year')
  assert.equal(monthEnd('2026-12'), '2026-12-31')
})

test('changing the unit keeps what the range covers as far as it can', () => {
  const changed = withMode(days('2026-03-10', '2026-05-20'), 'month')
  assert.deepEqual([changed.mode, changed.from, changed.to], ['month', '2026-03', '2026-05'])
  const finer = withMode(months('2026-03', '2026-05'), 'date')
  assert.deepEqual([finer.mode, finer.from, finer.to], ['date', '2026-03-01', '2026-05-31'])
  const yearly = withMode(months('2026-03', '2026-05'), 'year')
  assert.deepEqual([yearly.mode, yearly.from, yearly.to], ['year', '2026', '2026'])
})

test('an end before its start, an unfinished range, no categories or too long a report is refused', () => {
  assert.equal(problemWith('csv', all, 1), null)
  assert.equal(problemWith('csv', days('2026-05-01', '2026-04-30'), 1), 'dates')
  assert.equal(problemWith('csv', days('2026-05-01', '2026-05-01'), 1), null, 'one day is fine')
  assert.equal(problemWith('csv', months('2026-05', '2026-04'), 1), 'dates')
  assert.equal(problemWith('csv', months('2026-05', '2026-05'), 1), null, 'one month is fine')
  assert.equal(problemWith('csv', years('2027', '2026'), 1), 'dates')
  assert.equal(problemWith('csv', months('', '2026-05'), 1), 'dates', 'a range with no start')
  assert.equal(problemWith('csv', days('2026-05', '2026-05-02'), 1), 'dates', 'a month where a day is wanted')
  assert.equal(problemWith('json', { ...all, allCategories: false, categoryIds: [] }, 1), 'categories')
  assert.equal(problemWith('json', { ...all, allCategories: false, categoryIds: ['3'] }, 1), null)
  assert.equal(problemWith('pdf', all, MAX_REPORT_MONTHS + 1), 'months')
  assert.equal(problemWith('xlsx', all, MAX_REPORT_MONTHS + 1), null, 'only a report has a length limit')
})

test('an amount is written the way the app takes it, and what is not an amount is refused', () => {
  assert.deepEqual(['', ' ', '12', '12.5', '12,50', '0', '0.05', '007.10', ' 3.00 ', '9999999999.99'].map(amountText), ['', '', '12.00', '12.50', '12.50', '0.00', '0.05', '7.10', '3.00', '9999999999.99'])
  for (const bad of ['abc', '-1', '1.234', '1,2,3', '12.', '.5', '12 50', '1e3', '99999999999', '€5']) assert.equal(amountText(bad), null, bad)
})

test('choices left as they are send nothing, and the others go with the dates and the categories', () => {
  const choice: Choice = { ...months('2026-03', '2026-05'), allCategories: false, categoryIds: ['4'], kind: 'EXPENSE', minAmount: '5', maxAmount: '20,5', text: '  coffee ' }
  assert.deepEqual(selectionFor('csv', choice), { from: '2026-03-01', to: '2026-05-31', categoryIds: ['4'], kind: 'EXPENSE', minAmount: '5.00', maxAmount: '20.50', text: 'coffee' })
  for (const format of ['json', 'csv', 'xlsx'] as const) {
    const sent = selectionFor(format, { ...all, kind: '', minAmount: ' ', maxAmount: '', text: '   ' })
    assert.deepEqual(Object.keys(sent).sort(), ['categoryIds', 'from', 'to'], format)
  }
})

test('the parts of a file are sent only when some are left out, and the history only when nothing limits the file', () => {
  assert.equal('sections' in selectionFor('json', all), false)
  assert.equal('sections' in selectionFor('xlsx', all), false)
  assert.deepEqual(selectionFor('json', { ...all, sections: ['transactions', 'budgets'] }).sections, ['transactions', 'budgets'])
  // a limit leaves the history out of a JSON file, which is every part but that one
  const limited: Choice = { ...all, kind: 'INCOME' }
  assert.deepEqual(sectionsFor('json', limited), EXPORT_SECTIONS.filter((section) => section !== 'history'))
  assert.equal('sections' in selectionFor('json', limited), false, 'what is left is every part there can be')
  assert.deepEqual(selectionFor('json', { ...limited, sections: ['history', 'budgets'] }).sections, ['budgets'])
  // an Excel file has no sheet for the history
  assert.deepEqual(selectionFor('xlsx', { ...all, sections: ['history', 'budgets'] }).sections, ['budgets'])
  assert.equal('sections' in selectionFor('xlsx', { ...all, sections: EXPORT_SECTIONS.filter((section) => section !== 'history') }), false)
  // a CSV is the transactions and has no parts
  assert.equal('sections' in selectionFor('csv', { ...all, sections: ['budgets'] }), false)
})

test('an amount that is not one, the first above the second, or no part to put in is refused', () => {
  assert.equal(problemWith('csv', { ...all, minAmount: 'x' }, 1), 'amount')
  assert.equal(problemWith('json', { ...all, maxAmount: '1.234' }, 1), 'amount')
  assert.equal(problemWith('xlsx', { ...all, minAmount: '20', maxAmount: '19.99' }, 1), 'amount')
  assert.equal(problemWith('xlsx', { ...all, minAmount: '20', maxAmount: '20.00' }, 1), null, 'the same amount for both is fine')
  assert.equal(problemWith('xlsx', { ...all, minAmount: '9.99', maxAmount: '10' }, 1), null)
  assert.equal(problemWith('xlsx', { ...all, minAmount: '100', maxAmount: '9' }, 1), 'amount', 'compared as numbers, not as text')
  assert.equal(problemWith('csv', { ...all, sections: [] }, 1), null, 'a CSV has no parts')
  assert.equal(problemWith('json', { ...all, sections: [] }, 1), 'sections')
  assert.equal(problemWith('xlsx', { ...all, sections: ['history'] }, 1), 'sections', 'a workbook of the history would have no sheet')
  assert.equal(problemWith('json', { ...all, sections: ['history'], kind: 'INCOME' }, 1), 'sections', 'and a limit leaves the history out')
  assert.equal(problemWith('json', { ...all, sections: ['history'] }, 1), null)
  assert.equal(problemWith('json', { ...all, sections: ['budgets'] }, 1), null)
})

test('the bills shared with friends, or those that are not, are a limit of their own, sent when chosen', () => {
  assert.equal('split' in selectionFor('json', all), false)
  assert.equal(selectionFor('csv', { ...all, split: 'split' }).split, 'split')
  assert.equal(selectionFor('xlsx', { ...all, split: 'not_split' }).split, 'not_split')
  assert.equal('split' in selectionFor('pdf', { ...all, split: 'split' }), false, 'a report has no such choice')
  assert.equal(isLimited({ ...all, split: 'split' }), true)
  assert.equal(isLimited({ ...all, split: '' }), false)
})
