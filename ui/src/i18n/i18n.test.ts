import assert from 'node:assert/strict'
import { test } from 'node:test'
import { en } from './en.ts'
import { LANGUAGES, pickLanguage } from './languages.ts'
import { translateIn } from './translate.ts'

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
  'de common.name', 'de nav.budgets', 'de lock.1m',
  'de welcome.passphrase', 'de lock.passphrase',
  'fr common.type', 'fr common.description', 'fr nav.transactions', 'fr nav.budgets', 'fr lock.1m', 'fr lock.5m',
  'fr lock.15m', 'fr tx.title',
  'fr report.date', 'fr report.page',
  'de nav.export', 'de export.title', 'fr export.format', 'ro export.format',
  'es keys.groupGeneral', 'ro keys.groupGeneral', 'fr keys.groupActions', 'fr settings.notifications', 'fr debts.notifications',
  'de nav.sync', 'de phone.passphrase', 'de rec.confirmTitle', 'es rec.confirmTitle', 'fr rec.confirmTitle', 'it rec.confirmTitle', 'pt rec.confirmTitle', 'ro rec.confirmTitle', 'es common.no', 'it common.no',
  // the menu behind the logo
  'fr nav.menu', 'it nav.menu', 'pt nav.menu',
  'de sync.table.budgets', 'de sync.field.name', 'de sync.field.locale', 'fr sync.table.transactions', 'fr sync.table.budgets', 'fr sync.field.date', 'fr sync.field.description', 'fr sync.field.type', 'fr sync.field.locale', 'it sync.table.budgets', 'ro sync.field.locale',
  'fr nav.documents', 'fr docs.title', 'fr docs.kindImage', 'fr docs.kindTransaction',
  'fr sync.table.documents', 'fr sync.field.document', 'ro sync.table.documents', 'ro sync.field.document',
  'fr debts.note', 'fr sync.field.note', 'de debts.colStatus', 'ro debts.net', 'de common.details', 'de common.optional', 'fr docs.typeImage',
  'de code.show', 'fr code.show',
  'de accounts.bic', 'es accounts.bic', 'fr accounts.bic', 'it accounts.bic', 'pt accounts.bic', 'ro accounts.bic',
  'de sync.field.bic', 'es sync.field.bic', 'fr sync.field.bic', 'it sync.field.bic', 'pt sync.field.bic', 'ro sync.field.bic',
  'fr accounts.notes', 'fr sync.field.notes',
  'ro entry.tabTransfer',
  // a sentence made only of placeholders and symbols reads the same everywhere (docs/accounts-design.md, section 21)
  'de accounts.rateOf', 'es accounts.rateOf', 'fr accounts.rateOf', 'it accounts.rateOf', 'pt accounts.rateOf', 'ro accounts.rateOf',
  'de accounts.rateLine', 'es accounts.rateLine', 'fr accounts.rateLine', 'it accounts.rateLine', 'pt accounts.rateLine', 'ro accounts.rateLine',
  'de accounts.labelIn', 'es accounts.labelIn', 'fr accounts.labelIn', 'it accounts.labelIn', 'pt accounts.labelIn', 'ro accounts.labelIn',
  'de entry.inMain', 'it entry.inMain',
  // the stashes: a percent and a count of characters read the same in most languages, and "notes" is French as well (docs/stash-design.md, section 14)
  'de stashes.percent', 'es stashes.percent', 'it stashes.percent', 'pt stashes.percent', 'ro stashes.percent',
  'de stashes.noteCount', 'es stashes.noteCount', 'fr stashes.noteCount', 'it stashes.noteCount', 'pt stashes.noteCount', 'ro stashes.noteCount',
  'fr stashes.notes',
  // the columns of the list: "date" is French as well, and "d" is the short day in Spanish and Portuguese too
  'fr stashes.dateHead', 'es stashes.daysShort', 'pt stashes.daysShort',
  // "3.9 % per year": the rate and its words are placeholders (docs/loans-deposits-design.md, section 11)
  'de accounts.rateShown', 'es accounts.rateShown', 'fr accounts.rateShown', 'it accounts.rateShown', 'pt accounts.rateShown', 'ro accounts.rateShown',
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

test('a regional language falls back to its base language, then to English', () => {
  assert.equal(translateIn('de-AT', 'nav.settings'), 'einstellungen')
  assert.equal(translateIn('xx', 'nav.settings'), 'settings')
})

test('the best supported language is picked from a preference list', () => {
  assert.equal(pickLanguage(['ro-RO', 'en-US']), 'ro')
  assert.equal(pickLanguage(['ja-JP', 'pt-BR', 'en']), 'pt')
  assert.equal(pickLanguage(['ja-JP']), 'en')
  assert.equal(pickLanguage([]), 'en')
})
