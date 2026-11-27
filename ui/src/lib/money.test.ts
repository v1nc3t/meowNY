import assert from 'node:assert/strict'
import test from 'node:test'
import { amountString, convertAmount, convertBetween, formatMoney, fromMinor, invertAmount, moneyIn, parseRate, signedAmountString, sumAmounts, toMinor } from './money.ts'

test('transaction amounts pad to two decimals and reject empty, zero, and negative', () => {
  assert.equal(amountString('10', false), '10.00')
  assert.equal(amountString('10.5', false), '10.50')
  assert.equal(amountString('10.50', false), '10.50')
  assert.equal(amountString(' 0.5 ', false), '0.50')
  assert.equal(amountString('', false), null)
  assert.equal(amountString('0', false), null)
  assert.equal(amountString('0.0', false), null)
  assert.equal(amountString('0.00', false), null)
  assert.equal(amountString('-1', false), null)
  assert.equal(amountString('-1.50', false), null)
  assert.equal(amountString('10.555', false), null)
  assert.equal(amountString('01.00', false), null)
})

test('budget amounts allow zero and still reject negatives', () => {
  assert.equal(amountString('', true), null)
  assert.equal(amountString('0', true), '0.00')
  assert.equal(amountString('0.5', true), '0.50')
  assert.equal(amountString('-2', true), null)
})

test('rates follow the same rules as the backend', () => {
  for (const good of ['1.08', '160', '0.0063', '0.00000001', '123456789.12345678']) assert.ok(parseRate(good), good)
  for (const bad of ['', '0', '0.0', '-1', '1,5', '1.', '.5', '01.5', '1.123456789', '1e3', ' 1.2', '1234567890']) {
    assert.equal(parseRate(bad), null, bad)
  }
})

test('conversion rounds half up to the cent, matching the backend', () => {
  assert.equal(convertAmount('100.00', '1.08'), '108.00')
  assert.equal(convertAmount('0.05', '0.5'), '0.03', '2.5 cents rounds up')
  assert.equal(convertAmount('0.04', '0.5'), '0.02')
  assert.equal(convertAmount('0.01', '0.4'), '0.00')
  assert.equal(convertAmount('100000.00', '0.0063'), '630.00')
  assert.equal(convertAmount('9999999999.99', '2'), '19999999999.98', 'the preview shows it; the backend refuses it')
  assert.equal(convertAmount('100', '1.08'), null)
  assert.equal(convertAmount('100.00', 'abc'), null)
})

test('amounts convert to whole cents and back without loss', () => {
  assert.equal(toMinor('12.50'), 1250n)
  assert.equal(toMinor('-0.05'), -5n)
  assert.equal(toMinor('12.5'), null)
  for (const amount of ['0.00', '0.07', '-1250.99', '9999999999.99']) assert.equal(fromMinor(toMinor(amount) ?? 0n), amount)
  assert.equal(fromMinor(-1600n), '-16.00')
})

test('a comma is a decimal mark like the dot, and nothing else with a separator is an amount', () => {
  assert.equal(amountString('29,5', false), '29.50')
  assert.equal(amountString('29,50', false), '29.50')
  assert.equal(amountString(' 0,05 ', false), '0.05')
  for (const bad of ['29,', '29.', ',5', '1,234.50', '1.234,50', '29,555', '29,5,0', '29;50', '-29,50']) assert.equal(amountString(bad, false), null, bad)
})

test('a signed amount is an amount with an optional minus, and zero is allowed', () => {
  assert.equal(signedAmountString('120'), '120.00')
  assert.equal(signedAmountString('-120'), '-120.00')
  assert.equal(signedAmountString('1.5'), '1.50')
  assert.equal(signedAmountString('-1.5'), '-1.50')
  assert.equal(signedAmountString(' -0.05 '), '-0.05')
  assert.equal(signedAmountString('0'), '0.00')
  assert.equal(signedAmountString('0.00'), '0.00')
  // a comma is a decimal mark, as in every other amount
  assert.equal(signedAmountString('1,50'), '1.50')
  assert.equal(signedAmountString('-1,5'), '-1.50')
})

test('a signed amount refuses what the app refuses', () => {
  for (const bad of ['', ' ', '-', '--1', '-0', '-0.0', '-0.00', '-0,00', '+1', '+1.00', '1.555', '-1.555', '1.', '-.5', '- 5', '-01.00', '1e3', '−5', '1,234.50', 'abc', '5-']) {
    assert.equal(signedAmountString(bad), null, bad)
  }
})

test('a signed amount has the bounds of the app on both sides', () => {
  assert.equal(signedAmountString('9999999999.99'), '9999999999.99')
  assert.equal(signedAmountString('-9999999999.99'), '-9999999999.99')
  assert.equal(signedAmountString('10000000000'), null)
  assert.equal(signedAmountString('-10000000000.00'), null)
})

// -- an account in another currency: the amount in the app's currency from the native one (docs/accounts-design.md, section 21) -------------

test('the amount in the app\'s currency is the native amount divided by the rate, half up to the cent', () => {
  assert.equal(invertAmount('113.74', '5.00'), '22.75', '22.748')
  assert.equal(invertAmount('500.00', '5.00'), '100.00')
  assert.equal(invertAmount('0.01', '5'), '0.00', '0.002')
  assert.equal(invertAmount('0.00', '5'), '0.00')
  assert.equal(invertAmount('1.08', '1.08'), '1.00')
})

test('the inverse rounds half up at exactly half a cent, for rates below and above 1', () => {
  assert.equal(invertAmount('0.02', '4'), '0.01', '0.005 rounds up')
  assert.equal(invertAmount('0.01', '4'), '0.00', '0.0025 rounds down')
  assert.equal(invertAmount('0.03', '4'), '0.01', '0.0075 rounds up')
  assert.equal(invertAmount('0.01', '2'), '0.01', '0.005')
  assert.equal(invertAmount('0.05', '0.5'), '0.10', 'a rate below 1 makes more')
  assert.equal(invertAmount('0.01', '0.5'), '0.02')
  assert.equal(invertAmount('10.00', '0.5'), '20.00')
  assert.equal(invertAmount('100.00', '0.0063'), '15873.02', '15873.0158')
  assert.equal(invertAmount('0.01', '0.0063'), '1.59', '1.5873')
  assert.equal(invertAmount('1.00', '0.00000001'), '100000000.00', 'a rate of the smallest size')
  assert.equal(invertAmount('1.00', '123456789.12345678'), '0.00')
  // a third of a cent and two thirds
  assert.equal(invertAmount('0.01', '3'), '0.00')
  assert.equal(invertAmount('0.02', '3'), '0.01', '0.0067')
})

test('the inverse does not lose a cent to floating point, also for huge amounts', () => {
  assert.equal(invertAmount('9999999999.99', '1'), '9999999999.99')
  assert.equal(invertAmount('9999999999.99', '2'), '5000000000.00', '4999999999.995 is exactly half a cent: up')
  assert.equal(invertAmount('9999999999.99', '3'), '3333333333.33')
  assert.equal(invertAmount('9999999999.99', '7'), '1428571428.57', '1428571428.57 exactly')
  assert.equal(invertAmount('9999999999.99', '0.5'), '19999999999.98', 'the preview shows it; the app refuses it')
  assert.equal(invertAmount('9999999999.99', '0.00000001'), '999999999999000000.00', '9999999999.99 x 10^8, which a double cannot hold to the cent')
  // what a double would get wrong: 0.1 + 0.2 and 1.005 x 100
  assert.equal(invertAmount('1.01', '1.01'), '1.00')
  assert.equal(invertAmount('2.05', '2.5'), '0.82')
})

test('the inverse refuses a rate the app refuses, and an amount that is not one', () => {
  for (const rate of ['0', '0.0', '0.00', '-1', '-0.5', 'abc', '5,00', '', ' ', '5.', '.5', '01.5', '1e3', ' 5.00', '5.00 ', '+5', '1234567890', '1.123456789']) {
    assert.equal(invertAmount('10.00', rate), null, JSON.stringify(rate))
  }
  for (const amount of ['', '1', '1.5', '10,00', '-1.00', 'abc', '12345678901.00', '1.234', ' 1.00']) assert.equal(invertAmount(amount, '5.00'), null, JSON.stringify(amount))
})

test('euros to lei to euros gives the amount back at a rate of 1 or more, to the cent', () => {
  for (const rate of ['1', '1.08', '4.9712', '5.00', '5', '160', '123.456789']) {
    for (let cents = 1; cents <= 2000; cents += 7) {
      const euros = fromMinor(BigInt(cents))
      const lei = convertAmount(euros, rate) ?? ''
      assert.equal(invertAmount(lei, rate), euros, `${euros} at ${rate} (${lei})`)
    }
  }
})

test('between two currencies the amount goes through the app\'s currency in one exact step', () => {
  assert.equal(convertBetween('100.00', null, '5.00'), '500.00', 'main to foreign')
  assert.equal(convertBetween('500.00', '5.00', null), '100.00', 'foreign to main')
  assert.equal(convertBetween('100.00', null, null), '100.00', 'both in the app\'s currency')
  assert.equal(convertBetween('100.00', '5.00', '1.08'), '21.60', 'lei to dollars: 100 / 5.00 x 1.08')
  assert.equal(convertBetween('21.60', '1.08', '5.00'), '100.00')
  assert.equal(convertBetween('100.00', '5.00', '5.00'), '100.00', 'the same rate')
  assert.equal(convertBetween('0.01', '5.00', '1.08'), '0.00')
  // one rounding at the end, not one for each step: 33.33 lei to dollars is 7.19964 and not 6.67 x 1.08
  assert.equal(convertBetween('33.33', '5.00', '1.08'), '7.20')
  assert.equal(convertBetween('9999999999.99', '3', '3'), '9999999999.99', 'no float error')
  assert.equal(convertBetween('10.00', null, '5.00'), convertAmount('10.00', '5.00'), 'main to foreign is the conversion of the currency change')
  assert.equal(convertBetween('10.00', '5.00', null), invertAmount('10.00', '5.00'))
  for (const [from, to] of [['abc', null], [null, '0'], ['0', null], ['5,00', '1.08'], [null, '-1']] as const) assert.equal(convertBetween('10.00', from, to), null, `${from} ${to}`)
  assert.equal(convertBetween('10', '5.00', '1.08'), null)
})

test('money in another currency is written with that currency', () => {
  const lei = moneyIn('RON', 'en-GB')
  assert.match(lei('113.74'), /^RON\s113\.74$/)
  assert.match(lei('0.00'), /^RON\s0\.00$/)
  assert.match(lei('-12.50'), /^-RON\s12\.50$/, 'a negative amount')
  assert.match(lei('9999999999.99'), /^RON\s9,999,999,999\.99$/)
  assert.match(moneyIn('EUR', 'en-GB')('24.00'), /^€24\.00$/)
  assert.match(moneyIn('RON', 'ro-RO')('1234.50'), /^1\.234,50\sRON$/, 'the locale decides where the code goes')
  assert.equal(moneyIn('RON', 'en-GB')('12.00'), formatMoney('12.00', 'RON', 'en-GB'))
})

test('a currency the browser does not know does not throw: the amount behind its code', () => {
  assert.match(formatMoney('12.00', 'ZZZ', 'en-GB'), /ZZZ.12\.00/, 'a code nobody has a symbol for')
  assert.equal(formatMoney('12.00', 'EU', 'en-GB'), 'EU 12.00', 'two letters is no code')
  assert.equal(formatMoney('12.00', 'EURO', 'en-GB'), 'EURO 12.00')
  assert.equal(formatMoney('12.00', 'E1R', 'en-GB'), 'E1R 12.00')
  assert.equal(formatMoney('-12.50', 'EU', 'en-GB'), 'EU -12.50')
  assert.equal(formatMoney('12.00', '', 'en-GB'), '12.00', 'no currency at all: the amount alone, as before')
  assert.equal(formatMoney('12.00', 'RON', 'not a locale'), 'RON 12.00', 'a locale nobody has')
  assert.equal(formatMoney('12.00', 'RON', ''), 'RON 12.00')
  assert.equal(formatMoney('abc', 'RON', 'en-GB'), 'abc', 'not an amount')
  assert.equal(moneyIn('EU', 'en-GB')('1.00'), 'EU 1.00')
})

test('amounts add up in whole cents, signs and all', () => {
  assert.equal(sumAmounts(['8731.58', '92.25']), '8823.83')
  assert.equal(sumAmounts(['-319.90', '0.10']), '-319.80')
  assert.equal(sumAmounts(['0.10', '0.20']), '0.30', 'no floating point')
  assert.equal(sumAmounts([]), '0.00')
  assert.equal(sumAmounts(['x', '1.00']), '1.00', 'a text that is not an amount is 0')
})
