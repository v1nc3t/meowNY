import assert from 'node:assert/strict'
import { test } from 'node:test'
import { en } from './en.ts'
import { LANGUAGES, pickLanguage } from './languages.ts'
import { activeLanguage, interpolate, setActiveLanguage, t, translateIn } from './translate.ts'

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

test('every language has every key, and nothing extra', () => {
  const keys = Object.keys(en).sort()
  for (const language of LANGUAGES) {
    assert.deepEqual(Object.keys(language.messages).sort(), keys, language.code)
  }
})

test('every translation keeps the same {placeholders} as the English text', () => {
  for (const language of LANGUAGES) {
    for (const [key, text] of Object.entries(language.messages)) {
      assert.deepEqual(placeholders(text), placeholders(en[key as keyof typeof en]), `${language.code} ${key}`)
    }
  }
})

// Words that really are the same in that language (a loan word, or a unit such as "1 minute").
const SAME_AS_ENGLISH = new Set([
  'de common.name', 'de nav.budgets', 'de lock.1m', 'de budgets.title', 'de budgets.budget',
  'de welcome.passphrase', 'de lock.passphrase',
  'fr common.type', 'fr common.description', 'fr nav.transactions', 'fr nav.budgets', 'fr lock.1m', 'fr lock.5m',
  'fr lock.15m', 'fr tx.title', 'fr budgets.title', 'fr budgets.budget',
  'it budgets.budget',
  'fr report.date', 'fr report.page',
  'de nav.export', 'de export.title', 'fr export.format', 'ro export.format',
  'es keys.groupGeneral', 'ro keys.groupGeneral', 'fr keys.groupActions', 'fr settings.notifications',
  'de nav.sync', 'de phone.passphrase', 'de rec.confirmTitle', 'es rec.confirmTitle', 'fr rec.confirmTitle', 'it rec.confirmTitle', 'pt rec.confirmTitle', 'ro rec.confirmTitle', 'es common.no', 'it common.no',
  'de sync.table.budgets', 'de sync.field.name', 'de sync.field.locale', 'fr sync.table.transactions', 'fr sync.table.budgets', 'fr sync.field.date', 'fr sync.field.description', 'fr sync.field.type', 'fr sync.field.locale', 'it sync.table.budgets', 'ro sync.field.locale',
  'fr nav.documents', 'fr docs.title', 'fr docs.kindImage', 'fr docs.kindTransaction',
  'fr sync.table.documents', 'fr sync.field.document', 'ro sync.table.documents', 'ro sync.field.document',
  'fr debts.note', 'fr sync.field.note', 'ro debts.net', 'de common.details', 'de common.optional',
])

test('no translation is empty or an untranslated copy of the English text', () => {
  for (const language of LANGUAGES) {
    for (const [key, text] of Object.entries(language.messages)) {
      assert.ok(text.trim().length > 0, `${language.code} ${key} is empty`)
      if (language.code !== 'en' && text === en[key as keyof typeof en]) {
        assert.ok(SAME_AS_ENGLISH.has(`${language.code} ${key}`), `${language.code} ${key} was not translated`)
      }
    }
  }
})

test('the confirmation word the app asks for is never translated', () => {
  for (const language of LANGUAGES) {
    assert.match(language.messages['settings.deleteConfirm'], /DELETE/, language.code)
  }
})

test('the restore confirmation word is never translated either', () => {
  for (const language of LANGUAGES) {
    assert.match(language.messages['settings.restoreConfirm'], /RESTORE/, language.code)
  }
})

test('placeholders are filled in and unknown ones are left alone', () => {
  assert.equal(interpolate('use {count} characters', { count: 8 }), 'use 8 characters')
  assert.equal(interpolate('{a} and {b}', { a: 'x' }), 'x and {b}')
  assert.equal(interpolate('plain'), 'plain')
})

test('a regional language falls back to its base language, then to English', () => {
  assert.equal(translateIn('de-AT', 'nav.settings'), 'einstellungen')
  assert.equal(translateIn('xx', 'nav.settings'), 'settings')
})

test('t() follows the active language', () => {
  setActiveLanguage('ro')
  assert.equal(activeLanguage(), 'ro')
  assert.equal(t('nav.settings'), 'setări')
  setActiveLanguage('en')
  assert.equal(t('nav.settings'), 'settings')
})

test('the best supported language is picked from a preference list', () => {
  assert.equal(pickLanguage(['ro-RO', 'en-US']), 'ro')
  assert.equal(pickLanguage(['ja-JP', 'pt-BR', 'en']), 'pt')
  assert.equal(pickLanguage(['ja-JP']), 'en')
  assert.equal(pickLanguage([]), 'en')
})
