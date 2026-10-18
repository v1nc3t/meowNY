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
