import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { LANGUAGES } from '../i18n/languages.ts'
import { setActiveLanguage } from '../i18n/translate.ts'
import { debtsLabels } from './debtsLabels.ts'

/** The fields of `summary::Labels` in the Rust source, written the way the app sends them (camelCase). */
function rustFields(): string[] {
  const source = readFileSync(new URL('../../../src-tauri/src/debts/summary.rs', import.meta.url), 'utf8')
  const body = /pub struct Labels \{([\s\S]*?)\n\}/.exec(source)?.[1] ?? ''
  return [...body.matchAll(/^\s*pub (\w+): String,/gm)].map((m) => (m[1] ?? '').replace(/_(\w)/g, (_, c: string) => c.toUpperCase())).sort()
}

test('the labels are the fields the app expects, one for one (it refuses unknown and missing ones)', () => {
  setActiveLanguage('en')
  const fields = rustFields()
  assert.ok(fields.length > 15, 'the Rust struct was found')
  assert.deepEqual(Object.keys(debtsLabels()).sort(), fields)
})

test('every language gives every label, none empty, none too long, none with a control character', () => {
  for (const language of LANGUAGES) {
    setActiveLanguage(language.code)
    for (const [field, text] of Object.entries(debtsLabels())) {
      assert.ok(text.trim().length > 0 && [...text].length <= 120, `${language.code} ${field}`)
      assert.ok(!/\p{Cc}/u.test(text), `${language.code} ${field}`)
    }
  }
  setActiveLanguage('en')
})

test('the English words are the ones the command line uses', () => {
  setActiveLanguage('en')
  const labels = debtsLabels()
  assert.equal(labels.summaryTitle, 'Who owes what')
  assert.equal(labels.historyTitle, 'History of debts')
  assert.equal(labels.dueTitle, 'Due payments')
  assert.equal(labels.dueTotal, 'Total due')
  assert.equal(labels.paidPayment, 'recurring payment paid late')
  assert.equal(labels.duePayment, 'recurring payment overdue')
  assert.equal(labels.owedToYou, 'Owed to you')
  assert.equal(labels.settled, 'All settled.')
  assert.equal(labels.nobody, 'Nobody owes anything.')
  assert.equal(labels.lent, 'lent')
  assert.equal(labels.repaidToMe, 'paid back to me')
})
