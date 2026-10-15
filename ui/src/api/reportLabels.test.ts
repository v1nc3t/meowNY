import assert from 'node:assert/strict'
import { test } from 'node:test'
import '../i18n/languages.ts'
import { LANGUAGES } from '../i18n/languages.ts'
import { setActiveLanguage } from '../i18n/translate.ts'
import { periodLabel, reportLabels } from './reportLabels.ts'

// The fields the app's PDF writer expects, one for one (the Rust side refuses unknown or missing ones).
const FIELDS = [
  'amount', 'budgetVsActual', 'byMonth', 'category', 'date', 'expenses', 'income', 'name', 'noBudget', 'noData', 'other',
  'page', 'period', 'savings', 'savingsRate', 'spending', 'title', 'transactions',
]

test('every language gives every label, none empty, none too long for the PDF', () => {
  for (const language of LANGUAGES) {
    setActiveLanguage(language.code)
    const labels = reportLabels('2026')
    assert.deepEqual(Object.keys(labels).sort(), FIELDS, language.code)
    for (const [field, text] of Object.entries(labels)) {
      assert.ok(text.trim().length > 0 && text.length <= 120, `${language.code} ${field}`)
    }
  }
  setActiveLanguage('en')
})

test('labels start with a capital, except the title, which keeps its brand spelling', () => {
  setActiveLanguage('en')
  const labels = reportLabels('2026')
  assert.equal(labels.income, 'Income')
  assert.equal(labels.savingsRate, 'Savings rate')
  assert.equal(labels.title, 'meowNY report')
  setActiveLanguage('ro')
  assert.equal(reportLabels('2026').expenses, 'Cheltuieli')
  setActiveLanguage('en')
})

test('the period is written out in the number and date format of the person', () => {
  assert.equal(periodLabel('2026-10', '2026-10', 'en-US'), 'October 2026')
  assert.equal(periodLabel('2026-10', '2026-12', 'en-US'), 'October 2026 – December 2026')
  assert.equal(periodLabel('2026-03', '2026-03', 'de-DE'), 'März 2026')
  assert.equal(periodLabel('2026-03', '2026-03', 'en_US'), '2026-03', 'a locale the browser cannot use falls back to the month')
})
