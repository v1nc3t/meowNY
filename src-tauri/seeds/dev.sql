-- Demo data for development. `@today` is replaced with today's date, so the data always looks current:
-- sixteen months of history ending today.
-- Run through `cargo run --example dev_vault` (the app's own data) or `cargo run --example seed` (a separate
-- file). Never part of a release build. Money is integer minor units (cents).
--
-- What it is meant to show, so a screen or an export has something to chew on:
--   * budgets that change over time, a budget that is removed, a category that is often over budget and
--     categories with no budget at all
--   * recurring items that are paid, overdue, upcoming, paused and ended, and one paid with a different
--     amount than planned
--   * a deleted category that still has history
--   * awkward text: accents, Romanian and Japanese letters, commas, quotes, and cells that look like formulas
--   * enough rows for several pages of the list and the PDF
--   * debts: friends (one owes you, you owe one, one is settled, one was removed from the list and still owes), a ledger of loans and
--     repayments, bills shared with friends and a guest, and recurring items that are shared, overdue for several months, overdue with no
--     fixed amount, and due in a few days
--   * accounts: the main account renamed Current account (with an IBAN, an opening balance and a group), savings (a group and a note), cash,
--     a credit card that is in the red (a limit of 1000.00 as its virtual balance, and a BIC) and an archived account with history that net
--     worth leaves out; spending in cash and on the card, a recurring item on the card, a monthly transfer to savings and one that pays off
--     the card, and a friend who paid back in cash
--   * a currency per account: the Lei account (RON at 5.00 lei to the euro) with ten expenses and two incomes that each carry what the
--     account really paid and the amount in euro that the budgets count, and a transfer in each direction between it and the current account
--   * stashes: money set aside in an account for a goal, with adds that came with a transfer from the current account, one take with a note,
--     one that has reached its target, one with no target, an archived one that was spent, and one in the lei account in lei
--   * rates and debts: the savings earn 2.5 % a year, a mortgage (the Flat) and a loan (the Car) are owed with a rate and an installment, each
--     paid for the last three months with a payment split into interest and principal (made by seed.rs, through the core), and a term deposit
--     that is running in the savings (docs/loans-deposits-design.md, section 8)
-- Amounts and days come from simple arithmetic on the row numbers, so the data is the same every run.

INSERT OR IGNORE INTO settings (id, currency, locale, timezone, language) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin', 'en');

INSERT INTO categories (id, type, name) VALUES
  (1,  'INCOME',  'Salary'),
  (2,  'INCOME',  'Freelance'),
  (3,  'INCOME',  'Interest'),
  (4,  'EXPENSE', 'Rent'),
  (5,  'EXPENSE', 'Groceries'),
  (6,  'EXPENSE', 'Transport'),
  (7,  'EXPENSE', 'Eating out'),
  (8,  'EXPENSE', 'Entertainment'),
  (9,  'EXPENSE', 'Subscriptions'),
  (10, 'EXPENSE', 'Utilities'),
  (11, 'EXPENSE', 'Health'),
  (12, 'EXPENSE', 'Travel'),
  (13, 'EXPENSE', 'Gifts'),
  (14, 'EXPENSE', 'Școală și hobby-uri');

-- Soft-deleted category: its name is free to reuse, its history stays.
INSERT INTO categories (id, type, name, deleted_at) VALUES (15, 'EXPENSE', 'Gym', strftime('%Y-%m-%dT%H:%M:%fZ', @today, '-30 days'));

-- The interest of a loan payment is an expense, so it has a category of its own next to the income one of the same name (a name is free per type).
INSERT INTO categories (id, type, name) VALUES (16, 'EXPENSE', 'Interest');

-- Budgets. A change is a new row with a later effective_from; a NULL limit removes the budget from then on.
-- Health, Travel and Gifts never get one.
INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES
  ('EXPENSE', 'GLOBAL',   NULL, 230000, date(@today, 'start of month', '-15 months')),
  ('EXPENSE', 'GLOBAL',   NULL, 245000, date(@today, 'start of month', '-6 months')),
  ('INCOME',  'GLOBAL',   NULL, 360000, date(@today, 'start of month', '-15 months')),
  ('INCOME',  'CATEGORY', 1,    320000, date(@today, 'start of month', '-15 months')),
  -- Rent goes up seven months ago.
  ('EXPENSE', 'CATEGORY', 4,    90000,  date(@today, 'start of month', '-15 months')),
  ('EXPENSE', 'CATEGORY', 4,    95000,  date(@today, 'start of month', '-7 months')),
  -- Groceries limit raised eight months ago.
  ('EXPENSE', 'CATEGORY', 5,    45000,  date(@today, 'start of month', '-15 months')),
  ('EXPENSE', 'CATEGORY', 5,    50000,  date(@today, 'start of month', '-8 months')),
  ('EXPENSE', 'CATEGORY', 6,    7000,   date(@today, 'start of month', '-15 months')),
  ('EXPENSE', 'CATEGORY', 7,    20000,  date(@today, 'start of month', '-15 months')),
  -- Entertainment is dropped four months ago; the spending goes on.
  ('EXPENSE', 'CATEGORY', 8,    12000,  date(@today, 'start of month', '-15 months')),
  ('EXPENSE', 'CATEGORY', 8,    NULL,   date(@today, 'start of month', '-4 months')),
  ('EXPENSE', 'CATEGORY', 9,    4500,   date(@today, 'start of month', '-15 months')),
  ('EXPENSE', 'CATEGORY', 10,   25000,  date(@today, 'start of month', '-12 months'));

-- Recurring items. next_due_date is the first occurrence that has not been paid; the paid ones follow below.
--   1 rent, 2 and 3 subscriptions, 4 electricity (a bill with no fixed amount), 5 a yearly premium,
--   6 a cleaner every two weeks (three days overdue, paid a little more now and then), 7 salary,
--   8 a course that is paused, 9 a magazine that ended.
INSERT INTO recurring_transactions
  (id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active) VALUES
  (1, 4, 'Rent', 'Flat, paid by transfer', 95000,
      date(@today, 'start of month', '-15 months'), 'MONTH', 1,
      date(@today, 'start of month', '+1 month'), NULL, 1),
  (2, 9, 'Netflix', NULL, 1299,
      date(@today, 'start of month', '-15 months', '+11 days'), 'MONTH', 1,
      CASE WHEN date(@today, 'start of month', '+11 days') > @today THEN date(@today, 'start of month', '+11 days')
           ELSE date(@today, 'start of month', '+1 month', '+11 days') END, NULL, 1),
  (3, 9, 'Spotify', NULL, 1099,
      date(@today, 'start of month', '-15 months', '+4 days'), 'MONTH', 1,
      CASE WHEN date(@today, 'start of month', '+4 days') > @today THEN date(@today, 'start of month', '+4 days')
           ELSE date(@today, 'start of month', '+1 month', '+4 days') END, NULL, 1),
  (4, 10, 'Electricity', 'Varies with the meter', NULL,
      date(@today, 'start of month', '-15 months', '+19 days'), 'MONTH', 1,
      CASE WHEN date(@today, 'start of month', '+19 days') > @today THEN date(@today, 'start of month', '+19 days')
           ELSE date(@today, 'start of month', '+1 month', '+19 days') END, NULL, 1),
  (5, 10, 'Home insurance', 'Annual premium', 31200,
      date(@today, 'start of month', '-8 months', '+2 days'), 'YEAR', 1,
      date(date(@today, 'start of month', '-8 months', '+2 days'), '+1 year'), NULL, 1),
  (6, 10, 'Cleaning service', NULL, 3000,
      date(@today, '-171 days'), 'WEEK', 2,
      date(@today, '-3 days'), NULL, 1),
  (7, 1, 'Salary', 'Monthly pay', 324000,
      date(@today, 'start of month', '-15 months', '+24 days'), 'MONTH', 1,
      CASE WHEN date(@today, 'start of month', '+24 days') > @today THEN date(@today, 'start of month', '+24 days')
           ELSE date(@today, 'start of month', '+1 month', '+24 days') END, NULL, 1),
  (8, 14, 'Language course', 'Tuesday evening class', 3500,
      date(@today, 'start of month', '-10 months', '+14 days'), 'MONTH', 1,
      date(@today, 'start of month', '-4 months', '+14 days'), NULL, 0),
  (9, 9, 'Magazine', NULL, 650,
      date(@today, 'start of month', '-14 months', '+8 days'), 'MONTH', 1,
      date(@today, 'start of month', '-2 months', '+8 days'),
      date(@today, 'start of month', '-3 months', '+8 days'), 0);

-- The paid occurrences of those items. `i` is how many months ago; an occurrence that is still to come is left out.
-- Rent: the 1st, up from 900 to 950 seven months ago.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 4, 1, due, 'Rent', rent, rent, due, 'Flat, paid by transfer'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months') AS due,
             CASE WHEN i >= 8 THEN 90000 ELSE 95000 END AS rent FROM m);

-- Netflix on the 12th, Spotify on the 5th, the magazine on the 9th (until it ended), the course on the 15th
-- (until it was paused).
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 9, 2, due, 'Netflix', 1299, 1299, due, NULL
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+11 days') AS due FROM m) WHERE due <= @today;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 9, 3, due, 'Spotify', 1099, 1099, due, NULL
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+4 days') AS due FROM m) WHERE due <= @today;

WITH RECURSIVE m(i) AS (SELECT 3 UNION ALL SELECT i + 1 FROM m WHERE i < 14)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 9, 9, due, 'Magazine', 650, 650, due, NULL
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+8 days') AS due FROM m);

WITH RECURSIVE m(i) AS (SELECT 5 UNION ALL SELECT i + 1 FROM m WHERE i < 10)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 14, 8, due, 'Language course', 3500, 3500, due, 'Tuesday evening class'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+14 days') AS due FROM m);

-- Electricity on the 20th: no fixed amount, so nothing is planned and each payment is what the bill was (about 78),
-- a day or two late.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 10, 4, due, 'Electricity', 7800 + ((i * 53) % 61 - 30) * 10, NULL, min(date(due, '+' || (i % 3) || ' days'), @today), 'Varies with the meter'
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+19 days') AS due FROM m) WHERE due <= @today;

-- Salary on the 25th, up from 3200 to 3240 six months ago.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 1, 7, due, 'Salary', pay, pay, due, 'Monthly pay'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+24 days') AS due,
             CASE WHEN i >= 6 THEN 320000 ELSE 324000 END AS pay FROM m) WHERE due <= @today;

-- The yearly premium was paid once, and the cleaner twelve times (planned 30.00, now and then 35.00 for extra
-- hours); the next visit was three days ago.
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
VALUES (10, 5, date(@today, 'start of month', '-8 months', '+2 days'), 'Home insurance', 31200, 31200,
        date(@today, 'start of month', '-8 months', '+3 days'), 'Annual premium');

WITH RECURSIVE k(n) AS (SELECT 0 UNION ALL SELECT n + 1 FROM k WHERE n < 11)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 10, 6, due, 'Cleaning service', CASE WHEN n % 4 = 3 THEN 3500 ELSE 3000 END, 3000, due, NULL
FROM (SELECT n, date(@today, '-171 days', '+' || (14 * n) || ' days') AS due FROM k);

-- Everyday spending. `i` is months ago, `j` the n-th purchase that month.
-- Groceries: five shops a month, between 28 and 110.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15),
               s(j) AS (SELECT 1 UNION ALL SELECT j + 1 FROM s WHERE j < 5)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 5,
       CASE (i + j) % 4 WHEN 0 THEN 'Lidl' WHEN 1 THEN 'Aldi' WHEN 2 THEN 'Tesco' ELSE 'Farmers market' END,
       2800 + ((i * 37 + j * 101) * 7919) % 8200,
       d,
       CASE WHEN (i + j) % 9 = 0 THEN 'Weekly shop, includes cleaning supplies' END
FROM (SELECT i, j, date(@today, 'start of month', '-' || i || ' months', '+' || ((j * 6 + i) % 28) || ' days') AS d FROM m, s)
WHERE d <= @today;

-- Eating out: five meals a month, between 14 and 60, which goes over the 200 budget some months.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15),
               s(j) AS (SELECT 1 UNION ALL SELECT j + 1 FROM s WHERE j < 5)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 7,
       CASE (i * 3 + j) % 6 WHEN 0 THEN 'Pizza place' WHEN 1 THEN 'Sushi bar' WHEN 2 THEN 'Coffee and cake'
            WHEN 3 THEN 'Burger joint' WHEN 4 THEN 'Thai takeaway' ELSE 'Brunch' END,
       1400 + ((i * 41 + j * 73) * 6151) % 4600,
       d,
       CASE WHEN (i + j) % 11 = 0 THEN 'With friends' END
FROM (SELECT i, j, date(@today, 'start of month', '-' || i || ' months', '+' || ((j * 5 + i * 2 + 1) % 28) || ' days') AS d FROM m, s)
WHERE d <= @today;

-- Transport: the monthly pass, which uses the whole 70 budget, and a taxi now and then on top.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 6, 'Monthly transit pass', 7000, d, NULL
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+2 days') AS d FROM m) WHERE d <= @today;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15),
               s(j) AS (SELECT 1 UNION ALL SELECT j + 1 FROM s WHERE j < 2)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 6, 'Taxi', 900 + ((i * 29 + j * 67) * 4243) % 1800, d, CASE WHEN j = 2 THEN 'Late night' END
FROM (SELECT i, j, date(@today, 'start of month', '-' || i || ' months', '+' || ((j * 11 + i * 3) % 28) || ' days') AS d FROM m, s)
WHERE d <= @today AND (i + j) % 3 = 0;

-- Entertainment: two outings a month.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15),
               s(j) AS (SELECT 1 UNION ALL SELECT j + 1 FROM s WHERE j < 2)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 8,
       CASE (i + j) % 4 WHEN 0 THEN 'Cinema' WHEN 1 THEN 'Board game cafe' WHEN 2 THEN 'Concert tickets' ELSE 'Book store' END,
       1200 + ((i * 53 + j * 31) * 5393) % 4800,
       d,
       NULL
FROM (SELECT i, j, date(@today, 'start of month', '-' || i || ' months', '+' || ((j * 9 + i * 4 + 3) % 28) || ' days') AS d FROM m, s)
WHERE d <= @today;

-- Health, now and then, with no budget to measure it against.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15),
               s(j) AS (SELECT 1 UNION ALL SELECT j + 1 FROM s WHERE j < 2)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 11,
       CASE WHEN (i + j) % 2 = 0 THEN 'Pharmacy' ELSE 'Dentist' END,
       CASE WHEN (i + j) % 2 = 0 THEN 1250 + ((i * 17 + j) * 3511) % 2800 ELSE 6500 END,
       d,
       CASE WHEN (i + j) % 2 = 1 THEN 'Check-up and cleaning' END
FROM (SELECT i, j, date(@today, 'start of month', '-' || i || ' months', '+' || ((j * 7 + i) % 28) || ' days') AS d FROM m, s)
WHERE d <= @today AND (i * 2 + j) % 5 = 0;

-- The gym, until the category was deleted.
WITH RECURSIVE m(i) AS (SELECT 2 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 15, 'Gym membership', 3990, date(@today, 'start of month', '-' || i || ' months', '+3 days'), NULL FROM m;

-- Utilities besides the electricity: internet every month, water every third month.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 10, 'Internet', 3499, d, NULL
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+14 days') AS d FROM m) WHERE d <= @today;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 10, 'Water bill', 4300 + (i * 61) % 900, d, 'Quarterly'
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+21 days') AS d FROM m) WHERE d <= @today AND i % 3 = 2;

-- School and hobbies, in Romanian.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 14, 'Cursuri de pian', 4800, d, 'Lecție lunară'
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+20 days') AS d FROM m) WHERE d <= @today AND i % 2 = 0;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 14, 'Papetărie', 1450 + (i * 37) % 600, d, NULL
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+9 days') AS d FROM m) WHERE d <= @today AND i % 5 = 1;

-- Income besides the salary: freelance jobs every third month, interest every third month.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 2,
       CASE i % 9 WHEN 1 THEN 'Logo design' WHEN 4 THEN 'Website fixes' ELSE 'Consulting' END,
       CASE i % 9 WHEN 1 THEN 45000 WHEN 4 THEN 28000 ELSE 32000 END,
       d,
       'Invoice paid by client'
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+16 days') AS d FROM m) WHERE d <= @today AND i % 3 = 1;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description)
SELECT 3, 'Savings interest', 1200 + (i * 37) % 500, d, NULL
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+27 days') AS d FROM m) WHERE d <= @today AND i % 3 = 0;

-- Occasional bigger things, all well in the past: a trip, gifts and a bonus.
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description) VALUES
  (12, 'Flights to Lisbon',  31850, date(@today, 'start of month', '-5 months', '+8 days'),  'Return, two bags'),
  (12, 'Hotel Lisbon',       42000, date(@today, 'start of month', '-5 months', '+9 days'),  'Four nights'),
  (12, 'Train tickets',      4800,  date(@today, 'start of month', '-5 months', '+9 days'),  NULL),
  (12, 'Airport taxi',       3500,  date(@today, 'start of month', '-5 months', '+13 days'), NULL),
  (12, 'City break Prague',  28900, date(@today, 'start of month', '-10 months', '+11 days'), 'Long weekend'),
  (13, 'Birthday gift',      6500,  date(@today, 'start of month', '-9 months', '+4 days'),  NULL),
  (13, 'Wedding present',    15000, date(@today, 'start of month', '-7 months', '+19 days'), 'From all of us'),
  (13, 'Gifts for the family', 21000, date(@today, 'start of month', '-11 months', '+20 days'), NULL),
  (1,  'Year-end bonus',     150000, date(@today, 'start of month', '-9 months', '+14 days'), 'Paid with the salary');

-- The awkward ones, all within the last weeks. They are here to be looked at: in the list, in a CSV or Excel
-- file (the ones that look like formulas must come out as text), and in the PDF (the Japanese cannot be set in
-- the report's font and shows as question marks).
INSERT INTO transactions (category_id, name, amount_minor, payment_date, description) VALUES
  (8,  '=SUM(1+1)',                 999,    date(@today, '-2 days'),  '+44 letter, -10% promo, @Sam'),
  (13, '-5% voucher (used)',        1500,   date(@today, '-6 days'),  NULL),
  (5,  '@home delivery',            2350,   date(@today, '-9 days'),  'Order #4521'),
  (7,  'Lunch, "team" day',         6420,   date(@today, '-3 days'),  'Split 4 ways, paid cash'),
  (7,  'Café Müller',               1380,   date(@today, '-11 days'), 'Frühstück mit Anna'),
  (7,  '寿司 (sushi)',               2450,   date(@today, '-13 days'), 'ラーメンも食べた'),
  (14, 'Școala de dans',            5500,   date(@today, '-15 days'), 'Abonament lunar'),
  (11, 'Long name 0123456789 0123456789 0123456789 abcdefg', 1234, date(@today, '-20 days'),
       substr(replace(hex(zeroblob(128)), '00', 'note '), 1, 254)),
  (9,  'App store',                 99,     date(@today, '-4 days'),  NULL),
  (8,  'New laptop',                189900, date(@today, '-70 days'), 'Work machine, 24 month warranty'),
  -- Three on one day, to see how a day's rows are ordered.
  (5,  'Corner shop',               450,    date(@today, '-1 days'),  NULL),
  (6,  'Bus ticket',                280,    date(@today, '-1 days'),  NULL),
  (7,  'Coffee',                    320,    date(@today, '-1 days'),  'Flat white');


-- ---- debts ------------------------------------------------------------------------------------------------------------------------
-- Friends. Marta owes you, you owe Tomás (his part of the lunch below is taken off what you owe him), Priya has paid back everything,
-- including her half of the breakfast, and Sorin was taken off the list and still owes. (The names are not the ones the tests use.)
INSERT INTO people (id, name, archived) VALUES
  (1, 'Marta', 0),
  (2, 'Tomás', 0),
  (3, 'Priya', 0),
  (4, 'Sorin', 1);

-- The ledger: loans, repayments and a borrowing, none of it income or expense.
INSERT INTO ledger_entries (person_id, kind, amount_minor, entry_date, note) VALUES
  (1, 'LENT',         6000, date(@today, '-75 days'), 'Deposit for the trip'),
  (1, 'REPAID_TO_ME', 2000, date(@today, '-40 days'), NULL),
  (1, 'LENT',         1500, date(@today, '-12 days'), 'Concert ticket'),
  (1, 'REPAID_TO_ME', 13000, date(@today, '-6 days'), 'For the veg box'),
  (2, 'BORROWED',     5000, date(@today, '-30 days'), 'Concert tickets he bought'),
  (2, 'REPAID_BY_ME', 1000, date(@today, '-9 days'),  NULL),
  (3, 'LENT',         4000, date(@today, '-100 days'), 'Laptop repair'),
  (3, 'REPAID_TO_ME', 4000, date(@today, '-60 days'), NULL),
  (3, 'REPAID_TO_ME', 690,  date(@today, '-8 days'),  'Her half of the breakfast'),
  (4, 'LENT',         1000, date(@today, '-200 days'), 'Train ticket');

-- Bills shared with friends: the lunch on the table above split four ways (you, Marta, Tomás and a colleague who is not a friend), and a
-- breakfast with Priya. The bill stays what was paid; what it costs you is the rest.
INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor)
SELECT id, 1, NULL, 1605 FROM transactions WHERE name = 'Lunch, "team" day';
INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor)
SELECT id, 2, NULL, 1605 FROM transactions WHERE name = 'Lunch, "team" day';
INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor)
SELECT id, NULL, 'Colleague', 1605 FROM transactions WHERE name = 'Lunch, "team" day';
INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor)
SELECT id, 3, NULL, 690 FROM transactions WHERE name = 'Café Müller';

-- Recurring items for the debts screen and the dashboard.
--   10 a weekly veg box, shared with Marta half and half: due in five days, twelve weeks of payments with their parts
--   11 a yoga class that was not paid for two or three months (overdue, one occurrence each month)
--   12 a mobile top-up with no fixed amount, one or two months overdue
INSERT INTO recurring_transactions
  (id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active, split_my_weight) VALUES
  (10, 5, 'Veg box', 'Shared with Marta', 2400,
      date(@today, '+5 days', '-84 days'), 'WEEK', 1, date(@today, '+5 days'), NULL, 1, 1),
  (11, 8, 'Yoga class', 'Monthly pass', 3200,
      date(@today, 'start of month', '-6 months', '+4 days'), 'MONTH', 1,
      date(@today, 'start of month', '-2 months', '+4 days'), NULL, 1, NULL),
  (12, 10, 'Mobile top-up', 'Whatever the shop charges', NULL,
      date(@today, 'start of month', '-5 months', '+2 days'), 'MONTH', 1,
      date(@today, 'start of month', '-1 months', '+2 days'), NULL, 1, NULL);
INSERT INTO recurring_split_lines (recurring_transaction_id, person_id, weight) VALUES (10, 1, 1);

-- The veg box was paid on its day for the last twelve weeks; Marta's half is a part of each payment.
WITH RECURSIVE k(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM k WHERE n < 12)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 5, 10, due, 'Veg box', 2400, 2400, due, 'Shared with Marta'
FROM (SELECT date(@today, '+5 days', '-' || (7 * n) || ' days') AS due FROM k);
INSERT INTO transaction_split_lines (transaction_id, person_id, guest_name, amount_minor)
SELECT id, 1, NULL, 1200 FROM transactions WHERE recurring_transaction_id = 10;

-- The yoga class was paid four times, and then not. The top-up was paid three times, a day or two late, for what the shop charged.
WITH RECURSIVE m(i) AS (SELECT 3 UNION ALL SELECT i + 1 FROM m WHERE i < 6)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 8, 11, due, 'Yoga class', 3200, 3200, due, 'Monthly pass'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+4 days') AS due FROM m);

WITH RECURSIVE m(i) AS (SELECT 2 UNION ALL SELECT i + 1 FROM m WHERE i < 5)
INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description)
SELECT 10, 12, due, 'Mobile top-up', 1000 + (i * 7) % 5 * 250, NULL, date(due, '+' || (i % 3 + 1) || ' days'), 'Whatever the shop charges'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+2 days') AS due, i FROM m);


-- ---- accounts ---------------------------------------------------------------------------------------------------------------------
-- Everything above went into the main account, which every database has (id 1). It becomes the current account, with an IBAN, a group and
-- the balance it had when the history starts. The others are made here and some rows are moved into them. The credit card has a limit of
-- 1000.00 (its virtual balance) and a BIC, the savings a group and a note of two lines, and the archived account is left out of net worth.
UPDATE accounts SET name = 'Current account', number = 'IE29 AIBK 9311 5212 3456 78', opening_balance_minor = 240000,
                    opening_date = date(@today, 'start of month', '-16 months'), group_name = 'Everyday' WHERE id = 1;
INSERT INTO accounts (id, name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor) VALUES
  (2, 'Savings',     'SAVINGS',     NULL,                  150000, date(@today, 'start of month', '-16 months'), 0, 1,
      NULL, 'Emergency fund' || char(10) || 'Aim: six months of rent', 'Savings', 1, 0),
  (3, 'Cash',        'CASH',        NULL,                  25000,  date(@today, 'start of month', '-16 months'), 0, 2,
      NULL, NULL, NULL, 1, 0),
  (4, 'Credit card', 'CREDIT_CARD', '**** **** **** 4242', -24000, date(@today, 'start of month', '-16 months'), 0, 3,
      'COBADEFFXXX', NULL, NULL, 1, 100000),
  -- closed nine months ago: what was left went to the savings, so it holds nothing and is out of the lists, with its history
  (5, 'Old savings', 'SAVINGS',     NULL,                  120000, date(@today, 'start of month', '-16 months'), 1, 4,
      NULL, NULL, NULL, 0, 0);

-- The savings earn 2.5 % a year (the interest in it below is booked by hand, as the app has it).
UPDATE accounts SET rate_milli = 2500, rate_period = 'YEARLY' WHERE id = 2;

-- Cash: taxis, coffee and the lunch that was paid in cash. The card: the streaming plan (the recurring item and so its payments), the
-- trips, the laptop and the concert tickets. The interest goes to the savings. A friend paid back her half of the breakfast in cash.
UPDATE transactions SET account_id = 3 WHERE name IN ('Taxi', 'Coffee', 'Corner shop', 'Bus ticket', 'Coffee and cake', 'Lunch, "team" day');
UPDATE recurring_transactions SET account_id = 4 WHERE id = 2;
UPDATE transactions SET account_id = 4
 WHERE recurring_transaction_id = 2 OR name IN ('Flights to Lisbon', 'Hotel Lisbon', 'City break Prague', 'New laptop', 'Concert tickets', 'App store');
UPDATE transactions SET account_id = 2 WHERE name = 'Savings interest';
UPDATE ledger_entries SET account_id = 3 WHERE note = 'Her half of the breakfast';

-- Transfers: a monthly one to the savings the day after the salary, one that pays what was spent on the card the month before, and cash
-- from the bank every third month.
WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note)
SELECT 1, 2, 70000, d, 'Monthly saving'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+25 days') AS d FROM m) WHERE d <= @today;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note)
SELECT 1, 4, spent, d, 'Credit card payment'
FROM (SELECT date(@today, 'start of month', '-' || i || ' months', '+27 days') AS d,
             (SELECT COALESCE(SUM(t.amount_minor), 0) FROM transactions t
               WHERE t.account_id = 4 AND t.payment_date >= date(@today, 'start of month', '-' || (i + 1) || ' months')
                 AND t.payment_date < date(@today, 'start of month', '-' || i || ' months')) AS spent FROM m)
WHERE d <= @today AND spent > 0;

WITH RECURSIVE m(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note)
SELECT 1, 3, 20000, d, 'Cash from the bank'
FROM (SELECT i, date(@today, 'start of month', '-' || i || ' months', '+2 days') AS d FROM m) WHERE d <= @today AND i % 3 = 0;

-- The old savings account: paid into for six months, then emptied into the savings.
WITH RECURSIVE m(i) AS (SELECT 10 UNION ALL SELECT i + 1 FROM m WHERE i < 15)
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note)
SELECT 1, 5, 10000, date(@today, 'start of month', '-' || i || ' months', '+9 days'), 'Rainy day' FROM m;
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note)
VALUES (5, 2, 180000, date(@today, 'start of month', '-9 months', '+11 days'), 'Closing the old account');


-- ---- a currency per account -------------------------------------------------------------------------------------------------------------
-- The Lei account: an account in Romanian lei at 5.00 lei to the euro, opened eight months ago with 1,500.00 lei. Each row in it has `amount_minor`
-- in euro, which every budget and statistic counts, and `native_amount_minor`, what the account really paid or got, in lei; the lei are always
-- five times the euro, so every number is exact. No recurring item and no entry of the ledger is in it (they live only in accounts in euro).
INSERT INTO accounts (id, name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate) VALUES
  (6, 'Lei account', 'CHECKING', NULL, 150000, date(@today, 'start of month', '-8 months'), 0, 5, NULL, NULL, 'Everyday', 1, 0, 'RON', '5.00');

-- Ten expenses and two incomes, all months ago (the newest is a month back), so they are in the past on any day.
INSERT INTO transactions (category_id, name, amount_minor, native_amount_minor, payment_date, description, account_id) VALUES
  (5,  'Mega Image',          1600,  8000,  date(@today, 'start of month', '-7 months', '+3 days'),  NULL,                    6),
  (5,  'Kaufland',            2500,  12500, date(@today, 'start of month', '-6 months', '+8 days'),  'Weekend shop',          6),
  (7,  'Pizzerie',            1300,  6500,  date(@today, 'start of month', '-6 months', '+17 days'), NULL,                    6),
  (6,  'Bilet de tren',       900,   4500,  date(@today, 'start of month', '-5 months', '+4 days'),  'Cluj - București',      6),
  (2,  'Consulting in lei',   12000, 60000, date(@today, 'start of month', '-5 months', '+15 days'), 'Invoice paid in lei',   6),
  (7,  'Cafenea',             450,   2250,  date(@today, 'start of month', '-4 months', '+11 days'), NULL,                    6),
  (11, 'Farmacie',            550,   2750,  date(@today, 'start of month', '-4 months', '+20 days'), NULL,                    6),
  (8,  'Cinema',              700,   3500,  date(@today, 'start of month', '-3 months', '+6 days'),  NULL,                    6),
  (5,  'Piața',               800,   4000,  date(@today, 'start of month', '-2 months', '+9 days'),  NULL,                    6),
  (6,  'Taxi',                600,   3000,  date(@today, 'start of month', '-2 months', '+22 days'), 'Late night',            6),
  (5,  'Kaufland',            1800,  9000,  date(@today, 'start of month', '-1 months', '+5 days'),  NULL,                    6),
  (3,  'Dobândă în lei',      300,   1500,  date(@today, 'start of month', '-1 months', '+27 days'), NULL,                    6);

-- Euro out and lei in, and lei out and euro in: the first amount leaves the first account in its currency, the second arrives in the other.
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, to_amount_minor, transfer_date, note) VALUES
  (1, 6, 20000, 100000, date(@today, 'start of month', '-7 months', '+10 days'), 'Lei for the trip'),
  (6, 1, 50000, 10000,  date(@today, 'start of month', '-3 months', '+12 days'), 'Lei back to the current account');


-- ---- debts ------------------------------------------------------------------------------------------------------------------------
-- A mortgage and a loan, which are accounts you owe on: the opening balance is what was owed, below 0. Each has a rate and an installment.
-- Their payments (a transfer of the principal and an expense of the interest, three months of each) are made by `seed.rs` through
-- `accounts::pay::pay` after this file, because a payment is the core's to make.
INSERT INTO accounts (id, name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, rate_milli, rate_period, installment_minor) VALUES
  (7, 'Flat', 'MORTGAGE', NULL, -18000000, date(@today, '-2 years'), 0, 6, NULL, NULL, NULL, 1, 0, 3900, 'YEARLY', 95000),
  (8, 'Car',  'LOAN',     NULL, -900000,   date(@today, '-1 years'), 0, 7, NULL, NULL, NULL, 1, 0, 6500, 'YEARLY', 30000);


-- ---- stashes ----------------------------------------------------------------------------------------------------------------------
-- Money set aside inside an account for a goal; it never leaves the account, so no balance above moves (docs/stash-design.md). Five goals and a
-- deposit, each entry dated well inside the history and none on a day that could be after today, so every number below is the same on any day:
--   1 Holiday in Lisbon, in the savings: a target and a date seven months ahead, a monthly add of 100.00 (the first five with a transfer from the
--     current account, which is how the savings came to hold it) and one take of 40.00; 560.00 of 1200.00
--   2 New laptop, in the current account: no date, reached over three adds; 900.00 of 900.00
--   3 Rainy day, in the savings: no target, four adds; 500.00
--   4 Bike, archived: reached, and then taken out in full when the bike was bought; 0.00 of 450.00
--   5 Wedding gift, in the lei account: its amounts are in lei, two adds; 550.00 of 1000.00 lei
--   6 Term deposit, in the savings: a deposit at 3.1 % a year that started two months ago and matures ten months ahead (a term of a year), so it
--     is running on any day, with one add of 5000.00
-- Together they hold far less than their accounts have free, so no account is over-booked on any day.
INSERT INTO stashes (id, name, account_id, target_minor, target_date, notes, archived, position) VALUES
  (1, 'Holiday in Lisbon', 2, 120000, date(@today, 'start of month', '+7 months', '+14 days'), 'The next trip: a week, with the hotel still to pay', 0, 0),
  (2, 'New laptop',        1, 90000,  NULL, 'A personal one: the work machine is paid for',                                              0, 1),
  (3, 'Rainy day',         2, NULL,   NULL, NULL,                                                                                         0, 2),
  (4, 'Bike',              2, 45000,  NULL, 'Second-hand, for the commute',                                                               1, 3),
  (5, 'Wedding gift',      6, 100000, NULL, NULL,                                                                                         0, 4);

-- The adds of the holiday that came with money from the current account: a transfer each, which the entry remembers.
WITH RECURSIVE m(i) AS (SELECT 2 UNION ALL SELECT i + 1 FROM m WHERE i < 6)
INSERT INTO transfers (from_account_id, to_account_id, amount_minor, transfer_date, note)
SELECT 1, 2, 10000, date(@today, 'start of month', '-' || i || ' months', '+4 days'), 'Holiday in Lisbon' FROM m;

INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note, transfer_id)
SELECT 1, 10000, x.transfer_date, CASE WHEN x.transfer_date = date(@today, 'start of month', '-6 months', '+4 days') THEN 'start of the plan' END, x.id
FROM transfers x WHERE x.note = 'Holiday in Lisbon' ORDER BY x.transfer_date;

INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note) VALUES
  -- the last add was set aside from what the savings already held
  (1, 10000,  date(@today, 'start of month', '-1 months', '+4 days'),  'out of the freelance money'),
  (1, -4000,  date(@today, 'start of month', '-3 months', '+19 days'), 'deposit for the flat'),
  (2, 30000,  date(@today, 'start of month', '-5 months', '+9 days'),  'first third'),
  (2, 30000,  date(@today, 'start of month', '-4 months', '+9 days'),  NULL),
  (2, 30000,  date(@today, 'start of month', '-3 months', '+9 days'),  'there, with the pay of that month'),
  (3, 15000,  date(@today, 'start of month', '-8 months', '+12 days'), 'after the tax refund'),
  (3, 10000,  date(@today, 'start of month', '-6 months', '+12 days'), NULL),
  (3, 5000,   date(@today, 'start of month', '-4 months', '+12 days'), NULL),
  (3, 20000,  date(@today, 'start of month', '-2 months', '+12 days'), 'part of a consulting invoice'),
  (4, 15000,  date(@today, 'start of month', '-14 months', '+14 days'), 'birthday money'),
  (4, 15000,  date(@today, 'start of month', '-13 months', '+14 days'), NULL),
  (4, 15000,  date(@today, 'start of month', '-12 months', '+14 days'), 'last instalment'),
  (4, -45000, date(@today, 'start of month', '-11 months', '+5 days'),  'bought it'),
  (5, 30000,  date(@today, 'start of month', '-5 months', '+12 days'),  'for the wedding in Cluj'),
  (5, 25000,  date(@today, 'start of month', '-2 months', '+12 days'),  NULL);

-- The term deposit: a deposit has its rate, its period and its start, and its maturity is the target date (docs/loans-deposits-design.md, section 4.2).
INSERT INTO stashes (id, name, account_id, target_minor, target_date, notes, archived, position, kind, rate_milli, rate_period, start_date) VALUES
  (6, 'Term deposit', 2, NULL, date(@today, '+10 months'), NULL, 0, 5, 'DEPOSIT', 3100, 'YEARLY', date(@today, '-2 months'));
INSERT INTO stash_entries (stash_id, amount_minor, entry_date, note) VALUES
  (6, 500000, date(@today, '-2 months'), 'put on the term deposit');
