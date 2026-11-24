// The guide: what each part of meowNY does, and what it keeps where. Rules for this page:
// - How to do something: a numbered list of short steps, one action per step, button names in **bold** as the app shows them.
// - What something is or can do: a bullet list, one short item each.
// - A paragraph is one or two short sentences that introduce a section.
// This page is the user documentation; there is no Markdown copy of it. How sync, documents, debts, accounts, stashes, loans
// and deposits are built is in the design notes in docs/.

import { REPO, SPONSOR } from './render.mjs'
import { gallery } from './shots.mjs'

const doc = (name) => `${REPO}/blob/main/docs/${name}`
const RELEASES = `${REPO}/releases/latest`

/** The screenshots each section shows, under its text; missing files are left out. */
const PICTURES = {
  start: ['lock', 'phone-lock'],
  budgeting: ['dashboard', 'transactions', 'add-income', 'transactions-transfers', 'categories', 'recurring'],
  accounts: ['accounts', 'account'],
  stashes: ['stashes', 'stash', 'deposit'],
  documents: ['documents', 'preview-pdf', 'preview-csv'],
  debts: ['debts', 'friend', 'loans', 'split', 'bill-code'],
  phone: ['phone-sync', 'phone-menu', 'phone-dashboard', 'phone-transactions', 'phone-quick-add', 'phone-entry', 'phone-recurring', 'phone-debts', 'phone-categories', 'phone-accounts', 'phone-stashes'],
  backup: ['export'],
  privacy: ['settings'],
}

const SECTIONS = [
  {
    id: 'start',
    title: 'Getting started',
    blocks: [
      { t: 'p', text: 'meowNY runs on your computer (Linux, Windows) and your Android phone. Your data stays on your devices, encrypted.' },
      { t: 'note', kind: 'info', text: '**Pre-release.** Back up before you update. Nobody outside the project has reviewed the sync protocol yet.' },
      { t: 'h3', id: 'install', text: 'Install' },
      { t: 'p', text: `Every download is on [the latest release](${RELEASES}). Pick your system.` },
      { t: 'p', text: '**Linux** (Ubuntu, Debian and the like)' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_amd64.deb`.',
          'Install it: `sudo apt install ./meowNY_<version>_amd64.deb`. This adds the `meowny` command line too.',
          'Open **meowNY** from your app menu.',
        ],
      },
      { t: 'p', text: '**Other Linux** (AppImage)' },
      { t: 'ol', items: ['Download `meowNY_<version>_amd64.AppImage`.', 'Make it runnable: `chmod +x meowNY_<version>_amd64.AppImage`.', 'Run it.'] },
      { t: 'p', text: '**Windows**' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_x64-setup.exe`.',
          'Run it. Windows warns about an unsigned installer: choose **More info**, then **Run anyway**.',
          'Open **meowNY** from the Start menu.',
        ],
      },
      { t: 'p', text: '**Android**' },
      {
        t: 'ol',
        items: [
          'On the phone, download `meowNY_<version>_android.apk`.',
          'Open it. If Android asks, allow your browser or file manager to install apps.',
          'Open **meowNY**, then pair it with your computer ([your phone](#phone)).',
        ],
      },
      { t: 'note', kind: 'warn', text: `**iPhone and Mac: not available yet.** The App Store and Google Play need paid developer accounts. [Support meowNY on Ko-fi](${SPONSOR}) to help.` },
      { t: 'p', text: 'Check a download with `sha256sum -c SHA256SUMS --ignore-missing`. To build meowNY yourself: [develop](develop.html).' },
      { t: 'h3', id: 'updates', text: 'Update' },
      {
        t: 'ol',
        items: [
          'Open **settings**. The footer shows your version and **check for updates**.',
          `Press **check for updates**. Your browser opens [the latest release](${RELEASES}).`,
          'If it is newer, install it the same way as the first time. Your data stays.',
        ],
      },
      { t: 'p', text: 'meowNY never looks for updates by itself. On Android, [Obtainium](https://github.com/ImranR98/Obtainium) can fetch them.' },
      { t: 'h3', id: 'first-run', text: 'First run' },
      {
        t: 'ol',
        items: [
          'Press **get started**. With a backup, press **restore from a backup** instead ([restore](#restore)).',
          '**About you:** type your name (friends see it on shared bills) and choose a language.',
          '**Your money:** choose your currency, number and date format, and time zone.',
          '**Lock your data:** type a passphrase to be asked for it each time the app opens, or leave it empty. Press **create encrypted data file**.',
          '**Write down the recovery key.** It is shown once. It is the only way back in after a forgotten passphrase.',
          'Tick **i saved my recovery key**, then press **continue**.',
        ],
      },
      { t: 'h3', id: 'lock', text: 'Lock the app' },
      {
        t: 'ol',
        items: [
          'Open **settings** › **app lock**.',
          'Choose **lock after idle for**. On the computer you can also switch on **lock when minimized** and **lock when closed**.',
          'Press **save**.',
        ],
      },
      { t: 'p', text: 'To lock now, press **lock now**, `Ctrl/Cmd+L` or **lock** in the sidebar (on the phone, in the menu).' },
      { t: 'h3', id: 'forgot', text: 'Forgot the passphrase' },
      {
        t: 'ol',
        items: ['On the computer\'s lock screen, press **forgot my passphrase**.', 'Type your **recovery key**, and a new passphrase if you want one.', 'Press **unlock**.'],
      },
      { t: 'p', text: 'For a new recovery key, open **settings** › **recovery key** and press **make new key**.' },
      { t: 'h3', id: 'look', text: 'Theme, text size and language' },
      {
        t: 'ol',
        items: ['Open **settings** › **appearance**.', 'Pick a **theme**: **follow the system**, **light** or **dark**.', 'Move the **text size** slider, from 80 to 150%.'],
      },
      {
        t: 'ul',
        items: [
          'Theme and text size are kept per device.',
          'The language, number and date format and time zone are under **language and region**. Press **save** after a change.',
          'Seven languages: English, German, Spanish, French, Italian, Portuguese and Romanian.',
        ],
      },
      { t: 'h3', id: 'currency', text: 'Change the currency' },
      {
        t: 'ol',
        items: [
          'On the computer, open **settings** › **language and region**.',
          'Press **change** beside **currency**.',
          'Choose the **new currency**.',
          'Type the **rate**: what 1 unit of the old currency is worth in the new one, such as 1.08. The dialog shows what 100.00 becomes.',
          'Press **done**. The phone follows at the next sync.',
        ],
      },
      { t: 'p', text: 'To keep every amount as it is, tick **only change the label, keep amounts** instead. **change** is off while an account in another currency exists.' },
      { t: 'h3', id: 'data', text: 'Where your data lives' },
      {
        t: 'table',
        head: ['System', 'Folder'],
        rows: [
          ['Linux', '`~/.local/share/org.meowny.app/`'],
          ['macOS', '`~/Library/Application Support/org.meowny.app/`'],
          ['Windows', '`%APPDATA%\\org.meowny.app\\`'],
        ],
      },
      { t: 'p', text: 'The key is in your system keychain. Updating never touches this folder.' },
    ],
  },
  {
    id: 'budgeting',
    title: 'Budgeting',
    blocks: [
      { t: 'p', text: 'The pages you use most: **dashboard**, **transactions**, **categories** (your budgets are there) and **recurring**.' },
      { t: 'p', text: 'On the phone, the pages are in the menu behind the logo. The round **add** button holds each page\'s main action. A row\'s buttons are inside it: tap the row first ([on the phone](#on-phone)).' },
      { t: 'h3', id: 'dashboard', text: 'Dashboard' },
      {
        t: 'ul',
        items: [
          'Income, expenses, savings and the savings rate of a month, each against the month before.',
          'A year chart. Click a month to look at it.',
          'Spending by category, your budgets against what you spent, and what friends owe you or you owe.',
          'Recurring items that are overdue, due soon or split with friends.',
          '**net worth**, once you use accounts ([net worth](#net-worth)), and **stashes**, once you have one. Click a title to open the page.',
          'On an empty dashboard, **first steps**: add a transaction, set a budget, add a friend, pair your phone.',
        ],
      },
      { t: 'h3', id: 'transactions', text: 'Transactions' },
      { t: 'p', text: 'Spending is red with a minus. Income is green with a plus. A split bill shows only your part.' },
      {
        t: 'ul',
        items: [
          'The tabs **all**, **expenses** and **income** choose what the list shows.',
          '**transfers**: money you moved between your accounts ([transfers](#transfers)). Not there with one account.',
          'Columns: **name**, **category**, **account** (with more than one), **date** and **amount**.',
          'A last row adds up the period: how many, what came in, what went out, what is left.',
          'Click a row for its details: the split, the description, the documents. The pencil edits it and the bin deletes it.',
          'An edit has no tabs. To turn an expense into an income, delete it and add the other.',
        ],
      },
      { t: 'h3', id: 'add-entry', text: 'Add an expense, an income or a transfer' },
      { t: 'p', text: 'One dialog adds all three. It opens on the tab that fits the list you are on. `Alt+1`, `Alt+2` and `Alt+3` switch tabs and keep the amount, date and text.' },
      { t: 'p', text: '**Add an expense:**' },
      {
        t: 'ol',
        items: [
          'Open **transactions** and press **add transaction** (or `n`).',
          'Type a name and an amount, and choose a category. To make one, choose **add category** in the list.',
          'With more than one account, choose the **account** the money left.',
          'Check the payment date. It starts at today. You can type into it: for a month, `nov`, `november` and `11` all work.',
          'Optional: add a description, tick **split this bill** ([split a bill](#split)), or tick **attach documents** ([attach](#attach)).',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Add an income:**' },
      {
        t: 'ol',
        items: [
          'Press **add transaction** (or `n`) and choose **income**.',
          'Type a name and an amount, and choose an income category. With none yet, **add one** opens **categories** ([income categories](#income)).',
          'Under **into account**, choose where the money comes in. It is always asked.',
          'Check the date. An income cannot be split.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Add a transfer:** choose **transfer** and fill it in as in [move money between accounts](#transfers).' },
      { t: 'h3', id: 'find', text: 'Find transactions' },
      {
        t: 'ol',
        items: [
          'Open **transactions**. It shows one month. For a **year** or a **range** of days, press **filters** (the funnel beside **search**) and choose it under **period**.',
          'Step with the arrows (or `[` and `]`), or press the name to pick one. **current** brings you back. For a range, set **from** and **to**.',
          'Type part of a name or description in **search** (press `/`), or choose a **category** or an **account** under **filters**.',
          'Under the list, choose how many **rows** a page shows (10, 50 or 100).',
        ],
      },
      {
        t: 'ul',
        items: [
          'meowNY remembers the kind of period, the category, the account and the rows. A number on **filters** says how many are on.',
          'The **transfers** tab lists each transfer with its **date**, **from**, **to**, **amount** and **note**. Only the period and the account apply there.',
        ],
      },
      { t: 'h3', id: 'categories', text: 'Categories' },
      { t: 'p', text: '**categories** shows expenses and income for one month, with the **budget history** under them.' },
      {
        t: 'ul',
        items: [
          'Columns: **budget**, spent or earned, what is **left** (**to go** for income) and a bar of how much is **used**.',
          'The first row, **total**, covers the whole side. Its budget is the **monthly budget**.',
          'Spent is green within the budget and red above it. Earned is green above its budget.',
          'On the phone, three tabs: **expenses**, **income** and **budget history**. A strip shows the **monthly budget**, what was spent and what is left.',
        ],
      },
      { t: 'p', text: '**Add a category:**' },
      {
        t: 'ol',
        items: ['Open **categories** and press **add category** (or `n`).', 'Type a name and choose expense or income. Type a budget from this month on, if you want one.', 'Press **done**.'],
      },
      { t: 'p', text: '**Change the order:**' },
      {
        t: 'ol',
        items: [
          'Hold the grip at the start of a row and drag it. On the phone, first switch on **reorder categories** in the **⋯** menu.',
          'With the keyboard, focus the row and press `Alt` with the up or down arrow.',
          'Expenses and income keep their own order. It goes to your phone at the next sync.',
        ],
      },
      { t: 'p', text: '**Delete** asks for which months: only this month (the category stays), this month and later, or all months.' },
      { t: 'h3', id: 'budget', text: 'Budgets' },
      { t: 'p', text: 'A budget is set per month and category, and carries into later months. Expense and income budgets work the same.' },
      {
        t: 'ol',
        items: [
          'Open **categories** and choose the month.',
          'Press the pencil on a **total** row for the whole side, or on a category row.',
          'Type the amount. For a total, press **save**, or **clear** to remove it. For a category, press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          'Change a past month and meowNY asks how far it reaches: that month, that month and later, or every month.',
          'Category budgets cannot add up to more than the monthly budget.',
          'The **budget history** shows each month against its budgets. Click a month to look at it.',
        ],
      },
      { t: 'h3', id: 'income', text: 'Income categories' },
      { t: 'p', text: 'The income side of **categories** works like the expenses side. On the phone it is the **income** tab.' },
      {
        t: 'ol',
        items: ['Open **categories** and choose the **income** tab (on the phone) or look at its income side.', 'Press **add category** (or `n`), type a name and choose **income**.', 'Type a budget if you want one, then press **done**.'],
      },
      { t: 'h3', id: 'recurring', text: 'Recurring items' },
      { t: 'p', text: 'Rent, salary, subscriptions: what comes back. Two lists, **recurring expenses** and **recurring income**, each with what its running items come to **per month**.' },
      {
        t: 'ol',
        items: [
          'Open **recurring** and press **add recurring transaction** (or `n`).',
          'Choose the **expense** or **income** tab.',
          'Type a name, choose a category and type an amount. Leave the amount empty for a bill that **varies**: meowNY asks when you pay.',
          'Choose the account (an income asks under **into account**). Only accounts in your currency are listed.',
          'Set the **first due date** and **repeat every** some days, weeks, months or years. Tick **stops on a date** if it ends.',
          'Press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          'Columns: **category**, how often it **repeats**, **next due**, **status** and the amount.',
          'Status: **overdue** in red, **due today** in bold, **ongoing**, **paused** or **ended**.',
          'Each payment goes to the item\'s account, or to the main account if that one is archived.',
          'An edit has no tabs: an item keeps its kind.',
        ],
      },
      { t: 'p', text: '**Pay one:**' },
      {
        t: 'ol',
        items: [
          'When an item is due, press the check (**mark as paid**) on its row.',
          'Check the **amount paid** (type it for one that varies) and the date it was **paid on**.',
          'Press **done**. meowNY makes a transaction and moves the item to its next date.',
        ],
      },
      { t: 'p', text: 'In an open row:' },
      {
        t: 'ul',
        items: [
          '**skip**: passes over the next payment, with no transaction.',
          '**pause**: stops it coming due until you **resume** it.',
          '**end**: stops it for good.',
          '**code**: on a split item, one for each friend\'s next payment.',
        ],
      },
      { t: 'h3', id: 'due', text: 'Due payments' },
      { t: 'p', text: 'A payment past its day that was neither paid nor skipped is due. One due today is listed from tomorrow.' },
      {
        t: 'ol',
        items: [
          'Open **recurring** and its **due payments** tab.',
          'You see the total due and each missed payment, the oldest first.',
          'On a line, press **mark as paid** (check the amount and the date) or **skip**.',
        ],
      },
      { t: 'p', text: 'Payments made after their day are under **paid late**.' },
    ],
  },
  {
    id: 'accounts',
    title: 'Accounts',
    blocks: [
      { t: 'p', text: 'An account is a place your money is: a bank account, savings, cash, a credit card. Every transaction, recurring item and debt entry is in one account.' },
      {
        t: 'ul',
        items: [
          'You start with one **main account**. While you have only one, only an income asks for an account.',
          'An account can be in another currency ([an account in another currency](#account-currency)).',
          'A credit, loan or mortgage is on the **debts** tab of **debts** ([loans and mortgages](#loans)). Money set aside for a goal is a [stash](#stashes).',
          '**Not in meowNY:** reconciliation (correct the opening balance instead), recurring transfers, rules, and files attached to an account.',
        ],
      },
      { t: 'h3', id: 'add-account', text: 'Add an account' },
      {
        t: 'ol',
        items: [
          'Open **accounts** (or press `g` then `a`) and press **add account** (or `n`).',
          'Type a name and choose the type: **checking**, **savings**, **cash** or **credit card**.',
          'Leave the **currency** as it is, or type another, such as RON.',
          'Type the **opening balance** and the day it is **on**. Empty means 0.00. Start with a minus for a credit card you owe on.',
          'Optional: add an **account number**, or press **more** for a BIC, a group, a virtual balance and notes ([account details](#account-details)).',
          'Press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          'Columns: **type**, **this month**, **set aside** (what its stashes hold), **free** and **balance**.',
          '**free** is the balance less what is set aside. With a virtual balance it is **available**.',
          'An account in another currency shows its amounts in that currency. A crossed eye after a name means net worth leaves it out.',
          'Each group of more than one account ends in a **total** row, in your currency.',
          '**in accounts**, at the top, totals the accounts that count in net worth, archived ones too.',
        ],
      },
      { t: 'h3', id: 'look-account', text: 'Look at one account' },
      { t: 'p', text: 'Click an account\'s row. On the phone, tap it and press **open**. The page shows:' },
      {
        t: 'ul',
        items: [
          'Its name and balance, then its facts, one per line, and its notes.',
          'A chart of its balance over the year. Click a month to look at it.',
          'A month to step through (the arrows, or `[` and `]`; `t` is this month). It sets everything below.',
          '**spent by category** and **came in by category**, with a bar for each. Transfers and a split bill a friend paid are not in them.',
          'The statement: what moved the account, newest first, with the **balance after** each line. A transaction\'s line leads to the transaction, a friend\'s line to their history.',
        ],
      },
      { t: 'p', text: 'The title row has **add expense** and **add income**. **transfer**, **edit** and **book interest** are in the **⋯** menu. **back** returns to the list. An archived account has none of them: you can only **restore** it.' },
      { t: 'h3', id: 'account-details', text: 'Account details' },
      {
        t: 'ol',
        items: ['Open the account dialog (**add account**, or the pencil on a row) and press **more**.', 'Fill in what you want.', 'Press **done**.'],
      },
      {
        t: 'ul',
        items: [
          '**BIC**: the bank\'s code, up to 20 characters.',
          '**group**: puts accounts under a heading. Accounts without one come last, under **no group**.',
          '**virtual balance**: an amount added to the balance to show what is **available**, such as a card limit. A balance of −240.00 with 1000.00 virtual shows 760.00 available. It is never part of the balance, statement, chart or net worth.',
          '**count in net worth**: on at first. Switch it off to leave the account out. It gets a crossed eye.',
          '**notes**: up to 1000 characters, shown on its page.',
        ],
      },
      { t: 'h3', id: 'account-rate', text: 'A rate on an account' },
      { t: 'p', text: 'An **interest rate** is what a savings account earns, or what a loan costs ([loans and mortgages](#loans)). meowNY shows estimates and books nothing by itself.' },
      {
        t: 'ol',
        items: [
          'Open the account dialog (**add account**, or the pencil on a row).',
          'Type the **interest rate** as a percent, such as 2.5. Choose the **period**: **per month** or **per year**. Empty means no rate.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'The account then shows **return**: **≈** what it earns a month, as simple interest on today\'s balance.' },
      { t: 'p', text: '**Book the interest:**' },
      {
        t: 'ol',
        items: [
          'When the bank pays it, open the account and its **⋯** menu, and choose **book interest**.',
          'The add dialog opens on the income tab with the account and the amount filled in. Check it against your statement.',
          'Press **done**.',
        ],
      },
      { t: 'h3', id: 'balance-today', text: 'Set what it holds today' },
      {
        t: 'ol',
        items: [
          'Open the account dialog with the pencil on its row.',
          'Type what the account holds today in **balance today**. It can start with a minus.',
          'Press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          'meowNY moves the **opening balance** by the difference. Every earlier balance moves too. Your entries stay.',
          'A new account has no **balance today**: type its opening balance instead.',
        ],
      },
      { t: 'h3', id: 'account-currency', text: 'An account in another currency' },
      { t: 'p', text: 'Budgets, statistics and net worth use one currency: yours. An account can be in another, such as a lei account next to euro accounts.' },
      {
        t: 'ol',
        items: [
          'Open the account dialog (**add account**, or the pencil on a row).',
          'Under the type, type the **currency**: three letters, such as RON.',
          'A line appears: **1 EUR =** a field, then **RON**. Type the rate. If 1 EUR buys 5 lei, type 5.00.',
          'Type the **opening balance**, in lei. Press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          '**The rate is yours.** Nothing is fetched. Change it any time with the pencil. It changes no stored amount.',
          'It gives the account\'s value in your currency (**worth in EUR**), which **in accounts** and net worth add up.',
          'An entry has two amounts: **amount (RON)**, what moved the account, and **in EUR**, what budgets count. The euro amount starts from the rate. Type your own if you know it.',
          'A transfer between currencies has **amount (EUR)** and **arrives (RON)**. Each account moves by its own amount.',
        ],
      },
      { t: 'p', text: '**Not possible:**' },
      {
        t: 'ul',
        items: [
          'The **main account**, recurring items and debts stay in your currency.',
          'An account\'s currency cannot change once something is in it. Move its entries away first.',
          'Your own currency cannot change while such an account exists. Delete it, or put it back in your currency, first.',
        ],
      },
      { t: 'h3', id: 'account-order', text: 'Change the order' },
      {
        t: 'ol',
        items: ['On the computer, hold the grip at the start of an account row and drag it. Or focus the row and press `Alt` with the up or down arrow.', 'The order goes to your phone at the next sync.'],
      },
      { t: 'p', text: 'With groups, an account moves among its own group. The phone cannot change the order.' },
      { t: 'h3', id: 'archive', text: 'Archive, restore, delete' },
      {
        t: 'ul',
        items: [
          '**archive**: press it on a row and confirm. The account leaves the lists and forms. Its history and balance stay. If it holds money, move it out with a transfer first.',
          '**show archived**: switch it on in the **⋯** menu. **restore** brings one back.',
          '**delete**: an empty account has the bin in place of **archive**. An account with a stash cannot be deleted ([stash archive](#stash-archive)).',
          'The **main account** stays. You can edit it, not archive or delete it.',
        ],
      },
      { t: 'h3', id: 'balance', text: 'What the balance counts' },
      {
        t: 'ul',
        items: [
          'The opening balance, from its day on.',
          'Your income, the expenses you paid, and transfers.',
          'Money you lent, borrowed or got back from friends, or paid back to them.',
          'An entry dated after today counts from its day.',
          'A split bill you paid counts whole. One a friend paid counts for nothing until you pay them back. Budgets and statistics count only your part.',
        ],
      },
      { t: 'h3', id: 'transfers', text: 'Move money between accounts' },
      { t: 'p', text: 'A transfer is not income and not an expense. No budget or statistic counts it. You need two accounts. Start one from:' },
      {
        t: 'ul',
        items: [
          'the **transfer** tab of **add transaction**',
          '**transfer** beside **add account** on **accounts**',
          '**transfer** in the **⋯** menu of an account\'s page',
        ],
      },
      {
        t: 'ol',
        items: [
          'Choose the accounts **from** and **to**. Choosing the account of the other side swaps the two.',
          'Type the amount and check the date. It starts at today. Add a note if you like.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Find transfers on the **transfers** tab of **transactions** and in the statement of both accounts. The pencil edits one and the bin deletes it.' },
      { t: 'h3', id: 'net-worth', text: 'Net worth' },
      { t: 'p', text: 'On the dashboard, **net worth** is:' },
      {
        t: 'ul',
        items: [
          '**in accounts**: the accounts that **count in net worth**, archived ones too, each in your currency.',
          'Minus your **debts**: what you owe on loans, credits and mortgages ([loans and mortgages](#loans)).',
          'Plus what **friends** owe you, minus what you owe them.',
        ],
      },
    ],
  },
  {
    id: 'stashes',
    title: 'Stashes',
    blocks: [
      { t: 'p', text: 'A stash is money set aside in one account for a goal: a holiday, a laptop, a rainy-day fund. The money stays in the account, so no balance, net worth, budget or statistic changes.' },
      {
        t: 'ul',
        items: [
          '**goal**: save up to a target.',
          '**deposit**: money fixed for a term at a rate ([term deposits](#deposit)).',
          '**Not in meowNY:** a stash over several accounts, adds that happen by themselves, and a stash in another currency than its account.',
        ],
      },
      { t: 'h3', id: 'add-stash', text: 'Make a stash' },
      {
        t: 'ol',
        items: [
          'Open **stashes** (or press `g` then `h`) and press **new stash** (or `n`). Leave the switch on **goal**.',
          'Type a name, up to 50 characters. Two active stashes cannot share a name.',
          'Choose the **account** it lives in.',
          'Optional: type a **target**, and press **choose a date** for a **target date**.',
          'Optional: add **notes**, up to 1000 characters.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'To change a stash, open it and choose **edit** in the **⋯** menu. Its account can only change to one in the same currency with enough free money ([what is free](#stash-free)). The target cannot go below what is saved.' },
      { t: 'h3', id: 'look-stash', text: 'Look at your stashes' },
      {
        t: 'ul',
        items: [
          'Columns: **name**, **account**, **saved**, **target**, **progress**, **date** and **status**.',
          '**progress** is a bar and a percent. For a running deposit, it is how far through its term it is.',
          '**date** is a goal\'s target date, or a deposit\'s maturity.',
          '**status** is what to save **a month**, what is **left**, **reached** (green), **overdue** (red), or a deposit\'s state.',
          'The last row adds up what they hold and their targets. **total saved** is at the top of the page.',
          'The **account** and **kind of stash** lists above the table show one account or one kind only.',
          'A row has **+** (add) and **−** (take out). Click elsewhere on it to open the stash.',
          'Drag a row to reorder it, or focus it and press `Alt` with an arrow key.',
          'On the phone, a row shows the name, what is saved and a thin bar. A mark says **reached**, **overdue**, **matured** or **broken**.',
        ],
      },
      { t: 'p', text: 'A stash\'s page shows:' },
      {
        t: 'ul',
        items: [
          'Its name, what it holds, a bar of how far it is, its facts and its notes.',
          'A chart of what it held at the end of each month, with a dashed line for the target.',
          'The **history**: every add and take, newest first, with what the stash held **after** it.',
          'The account\'s line: what it holds, what is set aside and what is **free**.',
        ],
      },
      { t: 'p', text: 'Elsewhere: **accounts** has a **set aside** column, an account\'s page lists its stashes, and the dashboard has a **stashes** card.' },
      { t: 'h3', id: 'stash-money', text: 'Add money, take it out' },
      {
        t: 'ol',
        items: [
          'Press **+** or **−** on a row, or **add money** (or `n`) or **take out** on a stash\'s page. On the phone, **take out** is in the **⋯** menu.',
          'The dialog has two tabs, **add** and **take out** (`Alt+1`, `Alt+2`).',
          'Type the **amount**, or press a chip. For an add: **the rest** (to the target), **a month** (this month\'s share) or **all free**. For a take: **all** or **half**.',
          'Check the bar. The solid part is what the stash holds, the striped part what this changes. The line under it says what the stash would hold. If a limit breaks ([what is free](#stash-free)), it says why in red and **done** stays off.',
          'Check the date. It cannot be later than today. Add a **note** if you like.',
          'Optional: tick **bring it from another account** (add) or **send it to another account** (take out) and choose the account. A real transfer is made too.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Without that tick, no money moves: it was in the account already.' },
      { t: 'h3', id: 'stash-entry', text: 'Change or delete an entry' },
      {
        t: 'ol',
        items: [
          'Open the stash and click the line in its **history**. On the phone, tap it, then **edit entry**.',
          'Change the amount, the date or the note. An add stays an add and a take stays a take.',
          'Press **done**. To remove the entry, press **delete** and confirm.',
        ],
      },
      {
        t: 'ul',
        items: [
          'An entry that came with a transfer says so. **The transfer does not change.** Edit it on the **transfers** tab ([transfers](#transfers)).',
          'Deleting such an entry asks **delete the transfer too**. Without the tick, only the entry goes.',
          'Deleting a transfer on its own never deletes the entry.',
        ],
      },
      { t: 'h3', id: 'stash-free', text: 'What is free' },
      { t: 'p', text: 'The **free** money of an account is its balance minus what its stashes hold, archived ones too. With 1000.00 and stashes of 300.00 and 200.00, 500.00 is free. The limits:' },
      {
        t: 'ul',
        items: [
          'An add must fit in what is free.',
          'An add must not take the stash past its target.',
          'A take cannot take out more than is saved.',
          'The same limits apply when you change or delete an entry.',
          'You can still spend set-aside money. Free can then go below 0, and the account says **the stashes hold more than the account has**.',
          'Each device checks the limits when you make an entry. After a sync, two devices can together pass one. Nothing is lost.',
        ],
      },
      { t: 'h3', id: 'stash-target', text: 'The target' },
      {
        t: 'ul',
        items: [
          'With a target date, a stash shows how much to save **a month** until then, rounded up to the cent.',
          'Past its date and short of the target, a stash says **overdue** in red, with no amount a month.',
          'At or above its target, a stash says **reached**. It is never overdue.',
        ],
      },
      { t: 'h3', id: 'stash-archive', text: 'Archive, restore, delete' },
      {
        t: 'ul',
        items: [
          '**archive**: choose it in the **⋯** menu of a stash\'s page and confirm. The stash leaves the list. What it holds stays set aside. You can take money out, but not add.',
          '**show archived**: switch it on in the **⋯** menu of the list. **restore** brings one back, unless its name is taken.',
          '**delete**: choose it in the **⋯** menu of a stash\'s page, or press the bin on an archived row. The money stays in the account and becomes free. Transfers that came with entries stay.',
          'When a stash\'s account is archived, you can only take money out. To add again, move the stash with **edit**.',
          '**An account with a stash cannot be deleted.** Move or delete its stashes first.',
        ],
      },
      { t: 'h3', id: 'deposit', text: 'Term deposits' },
      { t: 'p', text: 'A deposit is money fixed at the bank for a term, at a rate. Its state is one of:' },
      {
        t: 'ul',
        items: [
          '**running**: until its maturity. It takes money.',
          '**matured**: past its maturity, waiting for the return.',
          '**booked**: the bank paid the return and you booked it.',
          '**broken**: you took the money out early. This is final.',
        ],
      },
      { t: 'p', text: '**Make a deposit:**' },
      {
        t: 'ol',
        items: [
          'Open **stashes**, press **new stash** and switch to **deposit**.',
          'Type a name and choose the account. Type the **interest rate** and its **period**.',
          'Check the **start date** (today) and the **maturity** (a year later). The maturity must be after the start.',
          'Press **done**, then **+** on its row to put the money in.',
        ],
      },
      { t: 'p', text: 'Its page shows the rate, dates, **days left**, **expected return** and **value at maturity**, in simple interest.' },
      { t: 'p', text: '**Book the return:**' },
      {
        t: 'ol',
        items: [
          'At maturity, press **return** on its row, in place of **+**.',
          'Check the amount (the expected return to start with), choose the **income category** and the day the bank paid it.',
          'Press **book return**. meowNY makes the income in the account and adds it to the deposit.',
        ],
      },
      {
        t: 'ul',
        items: [
          '**Break it:** while it runs, **−** is **break deposit**. It takes all it holds and ends the deposit for good.',
          '**Roll it over:** for a deposit that matured or was booked, press **edit** and set a new maturity after today.',
          '**Close it:** once it is booked or broken and empty, press **archive** on its page.',
          'A deposit is never **overdue**. Nothing is booked by itself.',
        ],
      },
    ],
  },
  {
    id: 'documents',
    title: 'Documents',
    blocks: [
      { t: 'p', text: 'Attach receipts and invoices to transactions, recurring items and debt entries. Each file is encrypted, up to 25 MiB. The same file added twice is stored once.' },
      { t: 'h3', id: 'add-document', text: 'Add a document' },
      {
        t: 'ol',
        items: [
          'Open **documents**.',
          'Press **add file**, or drag files onto the window. On the phone, press **take photo** in the **⋯** menu to use the camera.',
          'Click a row for its type, size, location and what it is attached to. The pencil changes its name and date.',
        ],
      },
      { t: 'h3', id: 'attach', text: 'Attach one to something' },
      {
        t: 'ol',
        items: [
          'Open the dialog of a transaction, a recurring item or a debt entry (a new one, or **edit**).',
          'In a transaction, tick **attach documents**. The other two show the area already.',
          'Press **add file** (or **take photo**), or **attach** to pick a document you have already.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'The documents show in the details of the item. On **documents**, a paper clip on a row opens the list of what it is attached to.' },
      { t: 'h3', id: 'view', text: 'Look inside a document' },
      {
        t: 'ul',
        items: [
          '**Preview** (the eye on a row) shows images, PDF, text, CSV, Word (.docx) and Excel (.xlsx) inside the app. Nothing in the file runs.',
          '**Save a copy** writes a plain, unencrypted copy where you choose. Other kinds of file can only be saved this way.',
        ],
      },
      { t: 'p', text: '**Open it in another program:**' },
      {
        t: 'ol',
        items: [
          'On **documents**, press the pencil on the row.',
          'Press **open**, and read the warning.',
          'Press **open** again. A plain copy goes to your system viewer. It is removed when the app locks or closes.',
        ],
      },
      { t: 'h3', id: 'phone-copy', text: 'Keep a copy on your phone' },
      { t: 'p', text: 'The computer is the archive. A document goes to the phone only if you ask.' },
      {
        t: 'ol',
        items: ['On the computer, open **documents** and click a document\'s row.', 'Tick **keep on phone**.', 'Sync the phone ([sync](#syncing)). The files come with it.'],
      },
      {
        t: 'ul',
        items: [
          'On the phone, a document that stays on the computer cannot be previewed. Its open row says so (**location**).',
          'A document added on the phone moves to the computer at the next sync.',
          'Backups can include the files. Exports list documents by name, never the files.',
        ],
      },
      { t: 'note', kind: 'info', text: 'Do not edit the `documents/` folder by hand: a file no document names is removed at the next unlock.' },
    ],
  },
  {
    id: 'debts',
    title: 'Debts and shared bills',
    blocks: [
      { t: 'h3', id: 'friends', text: 'Friends and the history' },
      { t: 'p', text: 'A friend is just a name: no phone, email or contacts. Each has a ledger outside your budget. Green means the friend owes you. Red means you owe them.' },
      { t: 'p', text: '**debts** has two tabs: **friends**, and **debts** for what you owe a bank ([loans and mortgages](#loans)). The friends tab shows:' },
      {
        t: 'ul',
        items: [
          'Your friends, each with who owes whom and the **balance**.',
          'The **history** under them: every change, newest first, with its **date**, the **change** and the **balance** after it. **older** and **newer** page through it.',
          'Click a friend to see only their lines. Click again, or press **back**, for everyone\'s.',
          'The pencil renames a friend. The bin **remove**s them from the list. Their balance and history stay.',
          '**show removed friends**, in the **⋯** menu, lists them again. **bring back** puts one back.',
        ],
      },
      { t: 'p', text: '**Add a friend:**' },
      { t: 'ol', items: ['Open **debts**, open the **⋯** menu and choose **add friend**.', 'Type a name.', 'Press **done**.'] },
      { t: 'p', text: '**Lend or borrow:**' },
      {
        t: 'ol',
        items: [
          'Open **debts** and press **lend** (money you gave; or press `n`) or **borrow** (money you got). On the phone, **borrow** is in the **⋯** menu.',
          'Choose the friend (or **add friend**), and type the amount and the date. Add a note if you like.',
          'With more than one account, choose the **account** the money left or arrived. Only accounts in your currency are listed.',
          'Press **done**.',
        ],
      },
      { t: 'h3', id: 'settle', text: 'Mark as paid' },
      {
        t: 'ol',
        items: [
          'Press the check (**mark as paid**) on a friend\'s row. It shows while the balance is not zero.',
          'Check the amount. It starts as the whole balance. Type less for a part payment.',
          'Set the date and add a note if you like. With more than one account, choose the **account**.',
          'Optional: choose **link a transaction** and pick the one this payment was for. It is only a reference.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'If you type more than the debt, the rest counts as a debt the other way. Tick **no need to pay back** to drop the difference.' },
      { t: 'h3', id: 'loans', text: 'Loans and mortgages' },
      { t: 'p', text: 'A **credit**, a **loan** or a **mortgage** is an account you owe on. Its balance is what you owe, below 0. They are on the **debts** tab of **debts**, with **total owed** above them.' },
      {
        t: 'ol',
        items: [
          'Open **debts**, choose the **debts** tab and press **new debt**.',
          'Type a name and choose **credit**, **loan** or **mortgage**.',
          'Type what is **owed** (without a minus) and since when.',
          'Optional: type the **interest rate** and its **period**, and the **installment** you pay each time.',
          'Press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          'Each debt shows what is **owed**, the **installment** and the **payoff**. The payoff is the month it is paid off, or **never** if the installment does not cover the interest.',
          'Open its row for the **interest rate** and the **interest per month**, in simple interest.',
          'A debt that is paid off says **paid off**. One you paid too much on says **overpaid**.',
          'Its page is an account\'s page ([look at one account](#look-account)), with **pay**. A debt holds no stash.',
        ],
      },
      { t: 'p', text: '**Pay one:**' },
      {
        t: 'ol',
        items: [
          'Press **pay** on its row or its page.',
          'Choose the account to **pay from**. The amount starts as the installment, and the **interest** as a month of interest on what is owed.',
          'Check both against your bank. With interest, choose the **interest category**. The dialog shows the **principal** and what is **owed after**.',
          'Press **done**. meowNY makes a transfer of the principal into the debt, and an expense of the interest.',
        ],
      },
      { t: 'h3', id: 'split', text: 'Split a bill' },
      {
        t: 'ol',
        items: [
          'Choose **split a bill** in the **⋯** menu of **debts**, or add an expense and tick **split this bill**. Only an expense can be split.',
          'Choose **how to split**: **equally**, **by percent**, **by shares** or **by amounts**.',
          'Add the friends who share it: choose one under **add friend**, or type a new name.',
          'For percent or amounts, type each part. Leave one field empty and it gets the rest. With two or more empty, the dialog shows **left to assign**. For shares, type a whole number each.',
          'If a friend paid, choose them under **paid by**.',
          'Press **done**.',
        ],
      },
      {
        t: 'ul',
        items: [
          'Budgets, statistics and exports count only your part. Each friend\'s part is added to what they owe.',
          'If a friend paid, you owe them your part, and the other friends owe them, not you.',
          'To change a split, edit the bill.',
          'A recurring item can be split too, with one person who pays. Each payment copies the split.',
        ],
      },
      { t: 'h3', id: 'bill-code', text: 'Send a friend their part' },
      {
        t: 'ol',
        items: [
          'On **transactions**, click the bill. Press **code** next to the friend\'s part. The friend who paid has no code.',
          'If meowNY lacks your name, it asks what your friend should see. **settings** › **name for friends** changes it.',
          'Show the QR code, or press **copy** and send the text. It holds that friend\'s part only.',
        ],
      },
      { t: 'h3', id: 'bill-take', text: 'Take in a bill code' },
      {
        t: 'ol',
        items: [
          'Open **debts** and its **⋯** menu. Choose **paste a code**. On the phone, choose **scan** and point the camera at the code.',
          'Paste the code and press **read the code**.',
          'Check who it is from, what it is for and your part. If the bill is in another currency, type the rate.',
          'Choose which friend it belongs to, or **new friend**.',
          'To count your part as an expense, tick **add my part to my expenses** and choose a category.',
          'Press **accept**, or **deny**. Nothing is written before you accept.',
        ],
      },
      { t: 'h3', id: 'debt-notifications', text: 'Debt notifications' },
      { t: 'p', text: 'A reminder comes after a debt last changed: every week, 2 weeks or month. The text holds a count, never a name or an amount.' },
      {
        t: 'ol',
        items: [
          'Open **debts**, open the **⋯** menu and choose **notifications**.',
          'Press a friend\'s name to open their history, press the x to clear that reminder, or press **clear all**.',
          'For the time and how often (**never**, every week, every 2 weeks, every month), open the **settings** tab there. Both are per device.',
        ],
      },
      { t: 'p', text: 'Clearing hides a reminder on this device only. The friend\'s next reminder comes back.' },
      { t: 'h3', id: 'statement', text: 'Save a statement' },
      {
        t: 'ol',
        items: [
          'On the computer, open **debts** and click a friend.',
          'Under **statement**, choose the **file type**: TXT, CSV or PDF.',
          'Press **save statement**, look at the preview, and save it where you choose. The file is not encrypted.',
        ],
      },
    ],
  },
  {
    id: 'phone',
    title: 'Your phone',
    blocks: [
      { t: 'p', text: 'The Android app keeps a full copy and works offline. It syncs over your home wifi, with no server.' },
      { t: 'h3', id: 'pairing', text: 'Pair once' },
      {
        t: 'ol',
        items: [
          'On the phone, open meowNY and press **get started**. Choose a language, type a passphrase twice (at least 12 characters) and press **create**.',
          'On the computer, open **settings** › **phone** and press **add a phone**. A QR code appears, with a countdown.',
          'On the phone, press **scan the code** and point the camera at it.',
          'The computer shows six digits. Type them on the phone.',
          'Wait for **paired with** and the name of the other device. Your data is copied to the phone.',
        ],
      },
      { t: 'h3', id: 'syncing', text: 'Sync' },
      {
        t: 'ol',
        items: [
          'On the computer, open **settings** › **phone**. Choose a **direction**: **both ways**, **phone to computer** or **computer to phone**.',
          'Press **sync with phone**. A QR code appears.',
          'On the phone, open the menu, choose **sync** and press **scan the code**.',
          'If the same thing changed on both devices, the conflicts show side by side. Keep one version, one by one or **all from** one device.',
          'A summary says what was added, changed or deleted.',
        ],
      },
      {
        t: 'ul',
        items: [
          'Both devices must be on the same network. If it keeps devices apart, use the phone\'s hotspot.',
          'If the phone cannot find the computer, open **another address…** under the direction and type the computer\'s address, like `192.168.1.20`.',
          'If the clocks differ, the sync still runs and both say so. The computer\'s time decides which change is newer.',
          'A passphrase locks the phone. Five wrong tries erase it. Pair again to get the data back.',
        ],
      },
      { t: 'h3', id: 'lost-phone', text: 'A lost phone' },
      { t: 'ol', items: ['On the computer, open **settings** › **phone**.', 'Press **remove** beside the phone, then **yes, remove**. It has to be paired again.'] },
      { t: 'h3', id: 'on-phone', text: 'On the phone' },
      {
        t: 'ul',
        items: [
          'The logo at the left of a page\'s title opens the menu: every page and **lock**. A swipe right from the left edge opens it too.',
          'The **⋯** button holds a page\'s second actions and switches such as **show archived**. A **?** beside a title explains the page.',
          'The round **add** button is on every page but settings and sync. It lists the page\'s main action (such as **new stash**), then **add transaction**, **add category**, **add friend** and **scan a bill**.',
          'Tap a row to open it: its facts, one per line, and its buttons.',
          'Press and hold a name that is cut off with "…" to see all of it.',
          '**keep on phone** brings a document from the computer.',
          'On the computer only: export, backups, the recovery key, pairing and the command line.',
          'Debt reminders ring at the time you chose, also when the app is closed.',
        ],
      },
    ],
  },
  {
    id: 'backup',
    title: 'Backup and export',
    blocks: [
      { t: 'p', text: 'A backup is one encrypted `.meowbak` file under a passphrase you choose. Exports are **not encrypted**.' },
      { t: 'h3', id: 'make-backup', text: 'Make a backup' },
      {
        t: 'ol',
        items: [
          'Open **settings** › **backup**.',
          'Under **manual backup**, type a backup passphrase twice. Tick **include the documents** if you want the files too.',
          'Press **back up now**, and choose where to save the file.',
        ],
      },
      { t: 'h3', id: 'schedule', text: 'Back up on a schedule' },
      {
        t: 'ol',
        items: [
          'Open **settings** › **backup**. Under **scheduled backups**, type a backup passphrase twice.',
          'Choose **how often**: every day, week or month.',
          'Press **choose folder and start**. meowNY keeps the newest 10 backups there.',
        ],
      },
      { t: 'p', text: '**stop scheduled backups** turns it off.' },
      { t: 'h3', id: 'restore', text: 'Restore a backup' },
      {
        t: 'ol',
        items: [
          'On a new install, press **restore from a backup** on the first screen. In an app with data, open **settings** › **backup** and go to **restore**.',
          'Type the backup passphrase. In **settings**, also type RESTORE to confirm. On a new install, add a passphrase for the app, or leave it empty.',
          'Press **restore from backup** (on a new install, **choose backup file and restore**) and choose the file.',
        ],
      },
      { t: 'p', text: 'Restoring replaces all your data. A failed restore changes nothing.' },
      { t: 'h3', id: 'new-computer', text: 'Move to a new computer' },
      { t: 'ol', items: ['Make a backup on the old computer.', 'Install meowNY on the new one ([install](#install)).', 'Restore the backup on the first screen.', 'Pair the phone again ([pair once](#pairing)).'] },
      { t: 'h3', id: 'export', text: 'Export' },
      {
        t: 'ol',
        items: [
          'Open **export**.',
          'Choose the **format**: JSON, CSV, Excel or PDF.',
          'Limit it if you like: the **period**, the **categories**, filters, and **what to include**.',
          'Check the preview of the file on the right.',
          'Press **export** and choose where to save it.',
        ],
      },
      { t: 'p', text: 'What the files hold:' },
      {
        t: 'ul',
        items: [
          'Each transaction, recurring item and debt entry names its account. In an account in another currency, it has its amount in that currency too.',
          'An unlimited JSON or Excel export also lists the accounts (with balances and details), transfers and stashes (with every entry). Excel has the sheets **Stashes** and **Stash entries**.',
          'A limited export names only the accounts of what it holds. It has no balances, numbers, transfers or stashes.',
          'The PDF report of every category adds a table of stashes, as on the last day of the period.',
          'Backups hold everything.',
        ],
      },
      { t: 'h3', id: 'delete-data', text: 'Delete all data' },
      {
        t: 'ol',
        items: ['Open **settings** › **your data**.', 'Type DELETE to confirm.', 'Press **delete all data**. This wipes the database, documents, keys and settings on this device.'],
      },
    ],
  },
  {
    id: 'privacy',
    title: 'Privacy and security',
    blocks: [
      {
        t: 'ul',
        items: [
          'No server, no account, no telemetry. Nobody but you holds your data.',
          'The database is encrypted (SQLCipher). Its key is stored wrapped twice: once for the keychain (and your passphrase), once for the recovery key.',
          'The app locks when idle and forgets the key. A locked app shows nothing, also to scripts.',
          'Logs and crash reports never hold amounts, descriptions, tokens or keys.',
          'Android: no system backup of the app\'s folder, no screenshots. iPhone: kept out of iCloud backups.',
        ],
      },
      { t: 'h3', id: 'stored', text: 'What is stored' },
      {
        t: 'table',
        head: ['What', 'Where', 'Protection'],
        rows: [
          ['Categories, budgets, accounts, transfers, stashes and their entries, transactions, recurring items, debts, the change history', '`meowny.db`', 'Encrypted'],
          ['Documents', '`documents/`, one sealed file each with a random name', 'Encrypted, per-file key'],
          ['Lock settings, the wrapped keys', '`vault.json`', 'No financial data'],
          ['Language, theme, text size, sidebar, this device\'s name, the name on bill codes, the time and how often of debt reminders', 'The window\'s local storage', 'No financial data, not encrypted'],
          ['The filters you chose (period kind, category, account, rows per page, document type, removed friends, archived shown); the last account and category used; the day a notice or reminder was shown; the reminders you cleared', 'The window\'s local storage', 'Numbers, days and choices only: no amounts, descriptions or names. Forgotten when all data is erased or restored'],
          ['The month or dates a page shows', 'Memory, while the app is open', 'Never written down'],
          ['A document open in another program', '`open/`, only while open', '**Not encrypted**, removed on lock and close'],
          ['Command line tokens', '`meowny.db` (only a hash); the token file of `meowny login`', 'Read-only by default, expire, revocable'],
        ],
      },
      { t: 'h3', id: 'leaves', text: 'What leaves the device' },
      {
        t: 'table',
        head: ['What', 'When'],
        rows: [
          ['Phone sync', 'When you start it. Straight to the paired phone on your home network, sealed with keys from the pairing. No relay or server'],
          ['Files: backups, exports, statements, copies of documents', 'Where you save them. Only backups are encrypted'],
          ['A bill code', 'When you show or copy it: one friend\'s part of one bill, not encrypted'],
          ['Notifications', 'Due items by name on the computer. Debt reminders by count only'],
          ['The clipboard', 'The recovery key, a new token or a bill code, only when you press copy'],
          ['A web page', 'When you press **check for updates** or another link in the footer. Your browser opens it. meowNY sends nothing'],
        ],
      },
      {
        t: 'ul',
        items: [
          'There are no other network calls. CI lists every place in the code that could open a connection and fails on a new one.',
          'The command line API listens on `127.0.0.1` only, is off until you turn it on, and needs a token. Exports, backups and copies of documents through it need your yes in the app each time.',
          '**delete all data** removes the database, `documents/`, `open/`, `vault.json` and the keychain entry. It leaves backups and exports you saved elsewhere, and the token file of `meowny login` (which stops working).',
        ],
      },
      { t: 'p', text: `How it is built: [sync](${doc('sync-design.md')}), [documents](${doc('documents-design.md')}), [debts](${doc('debts-design.md')}), [accounts](${doc('accounts-design.md')}), [stashes](${doc('stash-design.md')}).` },
    ],
  },
  {
    id: 'shortcuts',
    title: 'Keyboard shortcuts',
    blocks: [
      { t: 'p', text: 'Press `?` in the app to see them all.' },
      {
        t: 'table',
        head: ['Keys', 'Does'],
        rows: [
          ['`g` then `d` `t` `a` `c` `r`', 'dashboard, transactions, accounts, categories, recurring'],
          ['`g` then `f` `o` `e` `s` `h`', 'debts, documents, export, settings, stashes'],
          ['`n`', 'new'],
          ['`/`', 'search'],
          ['`[` `]`', 'previous, next month (or year)'],
          ['`t`', 'this month (or year)'],
          ['arrow keys', 'the next button, row or field that way. In a text field, left and right move the cursor first'],
          ['`j` `k`', 'next, previous row'],
          ['`Alt` + up or down arrow', 'move a category, an account or a stash one place'],
          ['`Alt+1` `Alt+2` `Alt+3`', 'the tabs of a dialog: expense, income, transfer (`Option` on a Mac). Recurring has the first two. A stash has add and take out'],
          ['`Esc`', 'close a row, a list or a dialog'],
          ['`Ctrl/Cmd+L`', 'lock'],
          ['`Ctrl/Cmd+B`', 'sidebar'],
        ],
      },
      { t: 'p', text: 'Rest the pointer on a name cut off with "…". A small box shows all of it and slides to show the rest.' },
    ],
  },
  {
    id: 'cli',
    title: 'Command line',
    blocks: [
      { t: 'p', text: '`meowny` lets scripts and AI agents use the running desktop app. The Linux `.deb` installs it. The release also has it on its own for Linux and Windows.' },
      { t: 'ol', items: ['Get `meowny`: the `.deb` installs it. Otherwise see [set it up](cli.html#quick-start).', 'In the app, open **settings** › **CLI access**. Turn it on and make a token. It is shown once.', 'Run `meowny login`, paste the token and press Enter.', 'Check: `meowny status`.'] },
      { t: 'p', text: 'Full reference: [command line](cli.html).' },
    ],
  },
]

/** The guide; `images` holds the screenshots whose files exist (build.mjs). */
export function buildGuide(images = new Set()) {
  const sections = SECTIONS.map((section) => {
    const html = gallery(PICTURES[section.id] ?? [], images)
    return html ? { ...section, blocks: [...section.blocks, { t: 'figures', html }] } : section
  })
  return {
    file: 'guide.html',
    title: 'meowNY guide',
    description: 'How meowNY works: install, budgets, documents, debts, the phone, backups, privacy and the command line.',
    lead: 'What each part of the app does, and what it keeps where.',
    footer: `meowNY is free software (AGPL-3.0). [Source](${REPO}) · [Support on Ko-fi](${SPONSOR}): it goes toward the App Store and Google Play.`,
    nav: [{ title: 'Guide', links: SECTIONS.map((s) => ({ id: s.id, text: s.title })) }],
    sections,
  }
}
