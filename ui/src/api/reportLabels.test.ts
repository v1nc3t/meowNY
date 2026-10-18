import assert from 'node:assert/strict'
import { test } from 'node:test'
import '../i18n/languages.ts'
import { LANGUAGES } from '../i18n/languages.ts'
import { setActiveLanguage } from '../i18n/translate.ts'
import { reportLabels } from './reportLabels.ts'

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
