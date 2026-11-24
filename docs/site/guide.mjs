// The guide: what each part of meowNY does, and what it keeps where. Every how-to is a numbered list of steps; what a thing
// is, and what it stores, is a short paragraph or list. This page is the user documentation; there is no Markdown copy of it.
// How sync, documents, debts, accounts, stashes, loans and deposits are built is in the design notes in docs/.

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
      { t: 'p', text: 'A budget app for your computer (Linux, Windows) and your Android phone. Your data stays on your devices, encrypted.' },
      { t: 'note', kind: 'info', text: '**Pre-release.** Back up before you update. Nobody outside the project has reviewed the sync protocol yet.' },
      { t: 'h3', id: 'install', text: 'Install' },
      { t: 'p', text: `Every download is on [the latest release](${RELEASES}). Pick your system below. Updating keeps your data.` },
      { t: 'p', text: '**Linux** (Ubuntu, Debian and the like)' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_amd64.deb`.',
          'Install it: `sudo apt install ./meowNY_<version>_amd64.deb`. This also adds the `meowny` command line.',
          'Open **meowNY** from your app menu.',
        ],
      },
      { t: 'p', text: '**Other Linux** (AppImage)' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_amd64.AppImage`.',
          'Make it runnable: `chmod +x meowNY_<version>_amd64.AppImage`.',
          'Run it.',
        ],
      },
      { t: 'p', text: '**To update** on Linux, install or run the new file the same way.' },
      { t: 'p', text: '**Windows**' },
      {
        t: 'ol',
        items: [
          'Download `meowNY_<version>_x64-setup.exe`.',
          'Run it. The installer is not signed yet, so Windows warns you: choose **More info**, then **Run anyway**.',
          'Open **meowNY** from the Start menu.',
        ],
      },
      { t: 'p', text: '**To update**, run the new installer.' },
      { t: 'p', text: '**Android**' },
      {
        t: 'ol',
        items: [
          'On the phone, download `meowNY_<version>_android.apk`.',
          'Open it. If Android asks, allow your browser or file manager to install apps.',
          'Open **meowNY**, then pair it with your computer: [your phone](#phone).',
        ],
      },
      { t: 'p', text: '**To update**, open the new APK, or add this repository to [Obtainium](https://github.com/ImranR98/Obtainium) to get each new version.' },
      { t: 'note', kind: 'warn', text: `**iPhone and Mac: not available yet.** There is no way to install meowNY on an iPhone or a Mac today. Publishing on the App Store and Google Play needs paid developer accounts. To help make that happen, [support meowNY on Ko-fi](${SPONSOR}).` },
      { t: 'p', text: 'Check a download with `sha256sum -c SHA256SUMS --ignore-missing`. To build meowNY yourself: [develop](develop.html).' },
      { t: 'h3', id: 'updates', text: 'Check for updates' },
      {
        t: 'ol',
        items: [
          'Look at the footer at the bottom of **settings** (the lock screen has it too). It shows your version, and **check for updates**.',
          `Press **check for updates**. Your browser opens [the latest release](${RELEASES}). If its version is newer than yours, download it and install it as above.`,
        ],
      },
      { t: 'p', text: 'meowNY does not look for updates by itself: the link only opens a web page.' },
      { t: 'h3', id: 'first-run', text: 'First run' },
      {
        t: 'ol',
        items: [
          'Press **get started**. (Have a backup? Press **restore from a backup** instead: [backup](#restore).)',
          '**About you:** type your name (friends see it on the bills you share with them) and choose a language.',
          '**Your money:** choose your currency, number and date format, and time zone.',
          '**Lock your data:** add a passphrase if you want the app to ask for it each time it opens. You can leave it empty. Press **create encrypted data file**.',
          '**Write down the recovery key.** It is shown once, and it is the only way back in after a forgotten passphrase. Copy it or download it.',
          'Tick **i saved my recovery key**, then press **continue**.',
        ],
      },
      { t: 'h3', id: 'look', text: 'Theme and text size' },
      {
        t: 'ol',
        items: ['Open **settings** and choose **appearance**.', 'Pick a **theme**: follow the system, light or dark.', 'Move the **text size** slider, from 80 to 150%. It applies when you let go.'],
      },
      { t: 'p', text: 'The theme and the text size are kept on each device. meowNY speaks seven languages (English, German, Spanish, French, Italian, Portuguese and Romanian); the language, the number and date format and the time zone are under **language and region**.' },
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
      { t: 'p', text: 'The key is in your system keychain (GNOME Keyring or KWallet, Windows Credential Manager, macOS Keychain). Updating the app never touches this folder.' },
    ],
  },
  {
    id: 'budgeting',
    title: 'Budgeting',
    blocks: [
      { t: 'p', text: 'The pages you use most: **dashboard**, **transactions**, **categories** (your budgets are there) and **recurring**. **accounts** shows where your money is: [accounts](#accounts). **stashes** sets some of it aside for a goal: [stashes](#stashes). On the phone the pages are in the menu behind the logo ([on the phone](#on-phone)), and the round **add** button replaces **add transaction** and **add category**.' },
      { t: 'h3', id: 'dashboard', text: 'Dashboard' },
      {
        t: 'ul',
        items: [
          'Income, expenses, savings and the savings rate of a month, with how much each changed from the month before. Click a month on the year chart to look at it; the arrows change the year, and arrow keys move along the chart.',
          'Spending by category, your budgets against what you spent, what friends owe you and what you owe, and the recurring items that are overdue, due soon or split with friends.',
          'Once you use accounts (you have more than one, or the main account has an opening balance): **net worth** ([net worth](#net-worth)). Click its title to open **accounts**.',
          'Once you have a stash: **stashes**, with the total saved and the first three. Click its title to open **stashes** ([stashes](#stashes)).',
          'A new, empty dashboard shows **first steps**: add a transaction, set a budget, add a friend, pair your phone.',
        ],
      },
      { t: 'h3', id: 'transactions', text: 'Transactions' },
      { t: 'p', text: 'Spending is red with a minus, income is green with a plus. A split bill shows only your part. The tabs above the list choose what it shows:' },
      {
        t: 'ul',
        items: [
          '**all**: income and expenses together.',
          '**expenses** or **income**: only that kind.',
          '**transfers**: money you moved between your own accounts ([transfers](#transfers)). Transfers are in no other tab. With only one account, this tab is not there.',
        ],
      },
      { t: 'p', text: 'Each row has a column for the **name**, the **category**, the **account** (with more than one) and the **date**, and the **amount** at the right. A narrower window leaves out the account, then the date, then the category. When every transaction of the period fits on the page, a last row adds them up: how many there are, what came in, what went out, and in bold what is left of the two. On the phone the rows are grouped by day, with the category in grey under the name, and the sums are a strip over the list.' },
      { t: 'p', text: 'Click a row (tap it on the phone) to open its details: how a bill is split, the description and the documents (the phone adds the account, the category and the date). The pencil edits a transaction and the bin deletes it; on the phone they are the buttons of the open row. An edit has no tabs: a transaction keeps its kind, so to turn an expense into an income, delete it and add the other.' },
      { t: 'h3', id: 'add-entry', text: 'Add an expense, an income or a transfer' },
      { t: 'p', text: 'One dialog adds all three. It has three tabs, and `Alt+1`, `Alt+2` and `Alt+3` choose one from anywhere in it. It opens on the tab that fits the list you are on: **income** on the income tab, **transfer** on the transfers tab, **expense** anywhere else.' },
      {
        t: 'ul',
        items: [
          '**expense** lists the expense categories and can be split with friends. With more than one account it asks which account the money left.',
          '**income** lists the income categories. It always asks which account the money comes **into**, also when you have only one.',
          '**transfer** moves money between two of your accounts. It is neither income nor expense. The tab is there with two accounts or more.',
        ],
      },
      { t: 'p', text: 'Changing tab keeps the amount, the date and the text you typed. The category, the split and the accounts of a transfer start again.' },
      { t: 'p', text: '**Add an expense:**' },
      {
        t: 'ol',
        items: [
          'Open **transactions** and press **add transaction** (or press `n`). On the phone, press the round **add** button, then **add transaction**. It opens on **expense**.',
          'Type a name and an amount, and choose a category. To make a new one, choose **add category** in the list.',
          'With more than one account, choose the **account** the money left. It starts at the one you used last.',
          'Check the payment date. It starts at today. Day, month and year are lists you can also type into: for a month, `nov`, `november` or `11` all work. Press Enter or leave the field to take it. A date that cannot exist stays red.',
          'Add a description if you like. It can run over several lines.',
          'Optional: tick **split this bill** to share it with friends ([split a bill](#split)), or **attach documents** to keep a receipt with it ([attach](#attach)).',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Add an income:**' },
      {
        t: 'ol',
        items: [
          'Press **add transaction** (or `n`) and choose the **income** tab (or press `Alt+2`).',
          'Type a name and an amount, and choose a category. The list holds income categories only. With none yet, the dialog says so, and **add one** takes you to the income side of **categories** ([income categories](#income)).',
          'Under **into account**, choose where the money comes in. It is always asked, also with one account.',
          'Check the payment date, and add a description or **attach documents** if you like. An income cannot be split.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Add a transfer:** choose the **transfer** tab (or press `Alt+3`) and fill it in as in [move money between accounts](#transfers).' },
      { t: 'h3', id: 'find', text: 'Find transactions' },
      {
        t: 'ol',
        items: [
          'Open **transactions**. The top of the page shows the month. To look at a **year** or a **range** of days, press **filters** (the funnel beside **search**) and choose it under **period**.',
          'For a month or a year, step with the arrows at the top (or `[` and `]`), or press its name to choose one. **current** brings you back to this month or year. For a **range**, choose the **from** and **to** days under **filters**.',
          'Type in **search** (press `/`) part of a name or description, or choose a **category** under **filters**.',
          'With more than one account, choose an **account** under **filters** to see only its transactions. **all accounts** shows everything again. Each row names its account.',
          'Under the list, choose how many **rows** a page shows (10, 50 or 100) and use the page buttons.',
        ],
      },
      { t: 'p', text: 'meowNY remembers the kind of period, the category, the account and the number of rows the next time. The dates are remembered only until you close the app. A number on the **filters** button says how many filters are on.' },
      { t: 'p', text: 'The **transfers** tab lists each transfer with its **date**, **from**, **to**, **amount** and **note**. Only the period and the account apply there (an account matches either side of a transfer); the category and the search are hidden. The pencil edits a transfer and the bin deletes it (on the phone, tap the row first).' },
      { t: 'h3', id: 'categories', text: 'Categories' },
      { t: 'p', text: 'Categories sort what you spend and what comes in. **categories** shows expenses and income side by side for one month (change it with the arrows at the top or `[` and `]`), with the **budget history** under them. In a narrow window the two sit one under the other. Each category row has its **budget**, what you spent or earned that month, what is **left** of the budget (**to go**, for income), and a bar of how much of it is **used**, with its percent. The first row, **total**, has the same columns for the whole side: its budget is the **monthly budget**. Spent is green within the budget and red above it, and an expense that went over has what is left in red and a red bar; earned is green above its budget, and its bar turns green. On the phone the page has three tabs: **expenses**, **income** and **budget history** (the history is there once you have some). A strip over the list has the **monthly budget**, what was spent and what is left. A category row has its name, what is left and a thin bar; tap it for its numbers and its buttons. More on [income categories](#income).' },
      {
        t: 'ol',
        items: [
          'Open **categories** and press **add category** (or press `n`).',
          'Type a name, and choose whether it is an expense or income. It starts as an expense, except on the phone\'s **income** tab, where it starts as an income.',
          'Type a budget from this month on, if you want one.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Change the order:**' },
      {
        t: 'ol',
        items: ['Find the grip at the start of a category row. On the phone, first switch on **reorder categories** in the **⋯** menu: the grips show, and the rows do not open while it is on.', 'Hold it and drag the row up or down. With the keyboard, focus the row and press `Alt` with the up or down arrow.'],
      },
      { t: 'p', text: 'The order is kept for expenses and for income apart, and goes to your phone with the next sync. **Delete** asks for which months: only this month (the category stays), this month and later, or all months.' },
      { t: 'h3', id: 'budget', text: 'Budgets' },
      { t: 'p', text: 'A budget is set per month and category, and carries into later months. Expense budgets and income budgets are both on **categories**, and they work the same.' },
      {
        t: 'ol',
        items: [
          'Open **categories** and choose the month.',
          'On the expenses or the income side, press the pencil on the **total** row (on the phone, the pencil beside the **monthly budget**).',
          'Type the amount and press **save**. To remove the budget, press **clear**.',
          'For one category, press the pencil on its row instead (on the phone, tap the row first), type the amount under **budget from … on**, and press **done**. You can rename it there too.',
        ],
      },
      { t: 'p', text: 'Change a budget of a past month and meowNY asks how far the change reaches: only that month, that month and later, or every month. Category budgets cannot add up to more than the monthly budget. On **categories**, the list of months under the budgets shows each month\'s spending and earnings against their budgets; click one to look at it (on the phone, tap a month and press **show this month**).' },
      { t: 'h3', id: 'income', text: 'Income categories' },
      { t: 'p', text: 'The income side of **categories** holds your income categories, their budgets and what came in this month. It works as the expenses side does. Earned is green above its budget. On the phone it is the **income** tab.' },
      {
        t: 'ol',
        items: [
          'Open **categories**. On the phone, choose the **income** tab.',
          'Press **add category** (or press `n`). Type a name, choose **income** as the type (on the phone\'s **income** tab it is chosen already), type a budget if you want one, and press **done**.',
          'Choose the month with the arrows (or `[` and `]`). For the budget of all income, press the pencil on the **total** row; for one category, press the pencil on its row ([budgets](#budget)).',
          'To change the order, hold the grip at the start of a row and drag, or use `Alt` with the up or down arrow (on the phone, switch on **reorder categories** in the **⋯** menu first).',
        ],
      },
      { t: 'h3', id: 'recurring', text: 'Recurring items' },
      { t: 'p', text: 'Rent, salary, subscriptions: what comes back. The page has two lists, **recurring expenses** and **recurring income**. Each heading shows what the running ones with a fixed amount come to **per month**. In a list, active items come first, then paused and ended ones, each group by category.' },
      {
        t: 'ol',
        items: [
          'Open **recurring** and press **add recurring transaction** (or press `n`; on the phone, choose it in the round **add** button\'s menu).',
          'Choose the **expense** or the **income** tab (or press `Alt+1` or `Alt+2`).',
          'Type a name and choose a category; the list holds the categories of that kind. Type an amount, or leave it empty for a bill with no fixed amount: it shows **varies** and asks for the amount when you pay it.',
          'Choose the account. An expense asks for it with more than one account; an income always asks, under **into account**. The list holds the accounts in your currency only.',
          'Set the **first due date**, and **repeat every** a number of days, weeks, months or years.',
          'Tick **stops on a date** if it ends, and set that date.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Each payment is made in the item\'s account. If that account is archived by then, the payment goes to the main account. An edit has no tabs: an item keeps its kind.' },
      { t: 'p', text: 'Each row has a column for the **category**, how often it **repeats**, the **next due** date and its **status**, with the amount at the right. The status is **overdue** in red, **due today** in bold, or **ongoing**, **paused** or **ended**. A narrower window leaves out columns, how often first, then the category. On the phone a row shows the name (with **overdue**, **paused** or **ended** after it when that is so), the next due date in grey under it, and the amount; tap it for the rest.' },
      { t: 'p', text: '**Pay one:**' },
      {
        t: 'ol',
        items: [
          'When an item is due, press the check (**mark as paid**) on its row. On the phone, tap the row, then **mark as paid**.',
          'Check the **amount paid** (type it for one that varies) and the date it was **paid on**.',
          'Press **done**. meowNY makes a transaction and moves the item to its next date.',
        ],
      },
      { t: 'p', text: 'Open a row (click it, or tap it) for more: **skip** passes over the next payment without a transaction, **pause** stops it coming due until you **resume** it (a paused item has a play button in the place of the check), and **end** stops it for good. A split item has a **code** for each friend\'s next payment.' },
      { t: 'h3', id: 'due', text: 'Due payments' },
      { t: 'p', text: 'A payment that is past its day, and was neither paid nor skipped, is a due payment. One that falls due today is not listed until tomorrow.' },
      {
        t: 'ol',
        items: [
          'Open **recurring** and its **due payments** tab. The tab shows how many there are.',
          'You see the total due and each missed payment, the oldest first. An income that did not come in is green.',
          'On a line, press **mark as paid** (check the amount and the date) or **skip**.',
        ],
      },
      { t: 'p', text: 'Payments that were paid after their day are listed under **paid late**, below.' },
    ],
  },
  {
    id: 'accounts',
    title: 'Accounts',
    blocks: [
      { t: 'p', text: 'An account is a place your money is: a bank account, savings, cash, a credit card. Every transaction, recurring item and debt entry is in one account. You start with one, the **main account**, and everything goes into it. While you have only one account, only an income asks for one, and you can leave this page alone.' },
      { t: 'p', text: 'The **accounts** page is the list below. An account can be in another currency than yours ([an account in another currency](#account-currency)). **Not in meowNY:** reconciliation (correct the opening balance instead), recurring transfers, rules and files attached to an account. Money you set aside for a goal is a [stash](#stashes). A credit, a loan or a mortgage is an account you owe on, on the **debts** tab of **debts** ([loans and mortgages](#loans)).' },
      { t: 'h3', id: 'add-account', text: 'Add an account' },
      {
        t: 'ol',
        items: [
          'Open **accounts** (or press `g` then `a`) and press **add account** (or press `n`; on the phone, choose it in the round **add** button\'s menu).',
          'Type a name and choose the type: **checking**, **savings**, **cash** or **credit card**.',
          'Leave the **currency** as it is, or type another, such as RON ([an account in another currency](#account-currency)).',
          'Add an **account number** if you like: an IBAN, or anything that helps you tell it apart.',
          'Type the **opening balance**: what the account held, and **on** which day. Empty means 0.00. It can start with a minus, for a credit card you owe on.',
          'Optional: press **more** for a BIC, a group, a virtual balance and notes ([account details](#account-details)).',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'The list shows each account with its **type**, what **this month** did to it, what its stashes hold (**set aside**, a dash for none), what is **free** (the balance less what is set aside; with a virtual balance, what is **available**) and its **balance**. A narrower window leaves out columns: set aside first, then the type and this month. An account in another currency shows its amounts in that currency. One that net worth leaves out has a crossed eye after its name. With groups, each group of more than one account ends in a **total** row, in your currency. **in accounts**, at the top, is the total of the accounts that count in net worth, archived ones too. The pencil edits an account. On the phone a row shows the name, its type in grey and the balance; tap it for the rest, one fact to a line (the account number, what this month did, set aside, free, the last activity, its [rate](#account-rate) and what it earns a month, what it is worth in your currency) and its buttons.' },
      { t: 'h3', id: 'look-account', text: 'Look at one account' },
      {
        t: 'ol',
        items: [
          'Click its row. On the phone, tap it and press **open**.',
          'The top shows its name and balance, then its facts, one to a line: the type, number, BIC, group, rate, what it earns a month, what is set aside and **free**, the last activity and, with a virtual balance, what is **available**. Its notes come after them.',
          'The chart shows its balance over the year, month by month. The arrows change the year; click a month to look at it.',
          'Under it, step through the months (the arrows, or `[` and `]`; `t` is this month). The month you choose sets everything below.',
          'Two lists show where the month went: **spent by category** and **came in by category**, each with its total and a bar for every category. They sit side by side, and one over the other on the phone. Transfers, and a split bill a friend paid, are not in them.',
          'Then the statement. Each line is something that moved the account, the newest first, with the **balance after** it. A transaction\'s line leads to the transaction, a friend\'s line to their history. A transfer\'s line has the pencil and the bin.',
          'The title row has **add expense** and **add income**. **transfer** (with two accounts or more), **edit** and **book interest** are in the **⋯** menu. The add buttons and **transfer** open the add dialog on their tab with this account already chosen. An archived account has none: you can only look at it, and **restore** it. On the phone **add expense** is a line of the round **add** button\'s menu, and the rest is in the **⋯** menu.',
          'Press **back** to return to the list.',
        ],
      },
      { t: 'h3', id: 'account-details', text: 'Account details' },
      {
        t: 'ol',
        items: ['Open the account dialog (**add account**, or the pencil on a row) and press **more**. It is open already for an account that has details.', 'Fill in what you want, as described below.', 'Press **done**.'],
      },
      { t: 'p', text: '**What each detail does:**' },
      {
        t: 'ul',
        items: [
          '**BIC**: the bank\'s code, up to 20 characters.',
          '**group**: a name that puts accounts under a heading in the list. The field offers the groups you use already.',
          '**virtual balance**: an amount added to the balance to show what is **available**, such as a credit card limit. A card with a balance of −240.00 and a virtual balance of 1000.00 shows 760.00 available. The virtual balance is never part of the balance, the statement, the chart or net worth. The **free** column of the list adds it.',
          '**count in net worth**: on at first. Switch it off for an account that net worth should leave out. It keeps its balance and its place in the list, and has a crossed eye after its name (**not in net worth**).',
          '**notes**: up to 1000 characters, shown on its page, under its facts.',
        ],
      },
      { t: 'p', text: 'When any account has a group, the list has a heading for each group, and the accounts without one come last, under **no group**. To move an account to another group, change its **group**.' },
      { t: 'h3', id: 'account-rate', text: 'A rate on an account' },
      { t: 'p', text: 'An account can have an **interest rate**: what a savings account earns, or what a loan costs ([loans and mortgages](#loans)). meowNY shows estimates from it and books nothing by itself.' },
      {
        t: 'ol',
        items: [
          'Open the account dialog (**add account**, or the pencil on a row).',
          'Type the **interest rate** as a percent, such as 2.5 (up to three decimals, 0 to 100), and choose the **period**: **per month** or **per year**. Leave it empty for no rate.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Its page, and its open row on the phone, then show **return**: **≈** what the account earns a month on what it holds today: the balance times the rate, a twelfth of it for a yearly rate, simple interest, rounded to the cent. When the bank pays it, open the account, open the **⋯** menu and choose **book interest**: the add dialog opens on the income tab with this account and that amount filled in. Check it against your statement, change it if it differs, and press **done**.' },
      { t: 'h3', id: 'balance-today', text: 'Set what it holds today' },
      {
        t: 'ol',
        items: [
          'Open the account dialog with the pencil on its row. A new account has no **balance today**: type its opening balance instead.',
          'Type what the account holds today in **balance today**. It can start with a minus. The **?** beside the title of the dialog says what it does: it sets the opening balance so the account holds this today.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'meowNY moves the **opening balance** by the difference, so every earlier balance moves by the same amount and your entries stay as they are. While **balance today** has a value, **opening balance** is shut: the two cannot be sent together. An opening day after today cannot be used with it. For an account in another currency, type it in that currency.' },
      { t: 'h3', id: 'account-currency', text: 'An account in another currency' },
      { t: 'p', text: 'meowNY keeps one currency for your budgets, statistics and net worth: yours, the one you chose when you started. An account can be in another, such as a lei account next to your euro accounts.' },
      {
        t: 'ol',
        items: [
          'Open the account dialog (**add account**, or the pencil on a row).',
          'Under the type, type the **currency**: three letters, such as RON. Leave your own currency there for an account in it.',
          'A line appears under it: **1 EUR =** a field, then **RON**. Type the rate as that sentence says: if 1 EUR buys 5 lei, type 5.00.',
          'Type the **opening balance**. The label names the currency, because it is now in lei. Press **done**.',
        ],
      },
      { t: 'p', text: '**The rate is yours.** Nothing is fetched from anywhere. It is used for two things: the value of the account in your currency (**worth in EUR**, on its page and in its open row on the phone), which **in accounts** and net worth add up, and the amount the forms suggest. Change it any time with the pencil. It changes no amount you stored, only that value.' },
      { t: 'p', text: '**An entry in such an account has two amounts:**' },
      {
        t: 'ul',
        items: [
          'In **add transaction**, an expense or an income in the account shows **amount (RON)**: what really left the account or reached it. Under it, **in EUR** starts from the rate while you have not typed in it. Type your own figure when you know it, from your bank, say.',
          'Budgets and statistics count the euro amount, **in EUR**, as they count any entry. The balance of the account moves by the lei.',
          'A transfer between accounts of different currencies has what leaves and what arrives: **amount (EUR)** and **arrives (RON)**. The second starts from the two rates until you type your own. Each account moves by its own amount.',
          'The transactions page shows the euro amount, and the amount in lei with its code in the details of the row. The page of the account, its statement, its chart and its breakdown are all in lei.',
        ],
      },
      { t: 'p', text: '**What is not possible:**' },
      {
        t: 'ul',
        items: [
          'The **main account** stays in your currency.',
          'Recurring items and debts stay in your currency: their account field lists only accounts in it.',
          'The currency of an account cannot change once something is in it (a transaction, a transfer, a recurring item or a debt entry). The dialog says to move its entries away first.',
          'Your own currency cannot be changed while such an account exists. In **settings**, the **change** button beside **currency** is off, and the **?** beside **language and region** says: an account in another currency exists; the app\'s currency cannot be changed. Delete the account, or move its entries away and put it back in your currency, first.',
        ],
      },
      { t: 'h3', id: 'account-order', text: 'Change the order' },
      {
        t: 'ol',
        items: ['On the computer, find the grip at the start of an account row.', 'Hold it and drag the row up or down. With the keyboard, focus the row and press `Alt` with the up or down arrow.'],
      },
      { t: 'p', text: 'With groups, an account moves among the accounts of its own group. The order goes to your phone with the next sync. It cannot be changed on the phone.' },
      { t: 'h3', id: 'archive', text: 'Archive, restore, delete' },
      {
        t: 'ul',
        items: [
          'For an account you no longer use, press **archive** on its row and confirm. It leaves the lists and the forms; its history and balance stay. If it still holds money, the question says how much: move it out with a transfer first if the account is closed.',
          'Open the **⋯** menu and switch on **show archived** (it says how many there are) to see the archived accounts, under their own heading. **restore** brings one back.',
          'An account with nothing in it has the bin in place of **archive**: **delete** removes it for good. A stash counts as something: [an account with a stash](#stash-archive) cannot be deleted.',
          'The **main account** always stays. You can rename and edit it, but not archive or delete it. New entries go there when no account is chosen.',
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
          'A split bill you paid counts whole, because the whole amount left your account; one a friend paid counts for nothing until you pay them back. Budgets and statistics still count only your part.',
          'The virtual balance is not part of it.',
        ],
      },
      { t: 'h3', id: 'transfers', text: 'Move money between accounts' },
      { t: 'p', text: 'A transfer moves money from one of your accounts to another. It is not income and not an expense, and no budget or statistic counts it. You need two accounts.' },
      {
        t: 'ol',
        items: [
          'Start it in one of these places: the **transfer** tab of **add transaction** (or `Alt+3`); **transfer**, next to **add account**, on **accounts** (on the phone it is in the **⋯** menu); or **transfer** in the **⋯** menu of an account\'s page, which starts from that account. On the phone, you can also tap an account in the list and press **transfer**.',
          'Choose the accounts **from** and **to**. If you choose the account the other side holds, the two swap.',
          'Type the amount and check the date. It starts at today. Add a note if you like.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'You find your transfers on the **transfers** tab of **transactions**, and as lines in the statement of each of the two accounts. To change or delete one, press the pencil or the bin on its line there (on the phone, tap the line first).' },
      { t: 'h3', id: 'net-worth', text: 'Net worth' },
      { t: 'p', text: 'On the dashboard, **net worth** is what is **in accounts**, minus your **debts** (what you owe on loans, credits and mortgages, [loans and mortgages](#loans)), plus what **friends** owe you, minus what you owe them. **in accounts** adds up only the accounts that **count in net worth**, archived ones too, each in your currency: an account in another currency counts at its rate. The virtual balance never counts.' },
    ],
  },
  {
    id: 'stashes',
    title: 'Stashes',
    blocks: [
      { t: 'p', text: 'A stash is money you set aside for something you save for: a holiday, a new laptop, a rainy-day fund. It lives in one account, and **the money stays in that account**. A stash only marks part of the balance as set aside, so it changes no balance, no net worth, no budget and no statistic.' },
      { t: 'p', text: 'Open **stashes** (or press `g` then `h`). A stash is a **goal** (save up to a target) or a **deposit**: money fixed for a term at a rate ([term deposits](#deposit)). **Not in meowNY:** a stash over several accounts, adds that happen by themselves, and a stash in another currency than its account.' },
      { t: 'h3', id: 'add-stash', text: 'Make a stash' },
      {
        t: 'ol',
        items: [
          'Open **stashes** and press **new stash** (or press `n`; on the phone, choose it in the round **add** button\'s menu). Leave the switch on **goal**; for a deposit, see [term deposits](#deposit).',
          'Type a name, up to 50 characters. Two stashes that are not archived cannot share a name.',
          'Choose the **account** the stash lives in. It is always asked, also with one account. A stash opened from an account\'s page starts with that account.',
          'Optional: type a **target**, how much you want to save. Empty means no target. For an account in another currency, the target is in that currency.',
          'Optional: press **choose a date** for a **target date**. The cross beside it takes the date away again.',
          'Add **notes** if you like, up to 1000 characters.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'To change a stash, open it and choose **edit** in the **⋯** menu. Its account can change only to an account in the same currency that has enough free money for what the stash holds ([what is free](#stash-free)). The target cannot go below what is saved.' },
      { t: 'h3', id: 'look-stash', text: 'Look at your stashes' },
      {
        t: 'ul',
        items: [
          'The list has a row for each stash, with a column for the **name**, the **account**, what is **saved**, the **target**, the **progress** (a bar and its percent; for a running deposit, how far through its term it is and the days left), the **date** (a goal\'s target date, a deposit\'s maturity) and the **status**: what to save **a month**, what is **left**, **reached** in green, **overdue** in red, or a deposit\'s state. A narrower window leaves out columns: the date first, then the target, the account and the bar. The last row adds up what they hold and their targets, when they are all in one currency. **total saved**, in your currency, is at the top of the page.',
          'With more than one account, the **account** list above the table shows the stashes of one account only, and the account column goes. With goals and deposits both, **kind of stash** shows one kind only.',
          'A row has **+** (add) and **−** (take out). Click anywhere else on it to open the stash.',
          'On the phone a row has the name and what is saved, with a thin bar of the progress under the name (a deposit says its state there). A mark after the name says **reached**, **overdue**, **matured** or **broken**. Tap a row for its facts and its buttons: add money (or the return of a deposit that matured), take out, open, edit and archive.',
          'On the computer, hold a row and drag it to change the order, or focus it and press `Alt` with an arrow key. This works while no account and no kind is chosen. The order goes to your phone with the next sync. It cannot be changed on the phone.',
          'The page of one stash has its name and what it holds on top, then a bar of how far it is, its facts one to a line (the account, the target, the percent, what is left, what to save a month, the date), the notes, a chart of what it held at the end of each month (the arrows change the year; a dashed line marks the target), and its **history**.',
          'The history lists every add and take, newest first: the date, a plus or a minus, the note, the account it came from or went to, the amount, and what the stash held **after** it.',
          'Under the history, the account\'s line says what it holds, how much of it is set aside and how much is **free**.',
        ],
      },
      { t: 'p', text: 'The **accounts** list shows what is **set aside** in each account in a column of its own. On an account\'s page, **stashes** lists them as small cards, with **new stash** for that account. The dashboard has a **stashes** card once you have one.' },
      { t: 'h3', id: 'stash-money', text: 'Add money, take it out' },
      {
        t: 'ol',
        items: [
          'Press **+** or **−** on a row (on the phone, tap the row and press the add or take-out button). On a stash\'s page, press **add money** (or `n`) or **take out**; on the phone, **add money** is a line of the round **add** button\'s menu and **take out** is in the **⋯** menu. The dialog has two tabs, **add** and **take out**, and `Alt+1` and `Alt+2` choose one. Changing tab keeps the amount, the date and the note.',
          'Type the **amount**, or press a chip under it: **the rest** (what is left to the target), **a month** (what the target date asks of this month), **all free** (everything that is free) for an add; **all** or **half** of what is saved for a take. Only the chips that make sense for the stash are shown.',
          'Look at the bar under the chips. The solid part is what the stash holds now, the striped part what this adds or takes, and the line says what it would hold: **after this: 550.00 of 1200.00 · 45%**. If the amount breaks a limit ([what is free](#stash-free)), the line says why in red and **done** stays off.',
          'Check the date. It starts at today and cannot be after today. Add a **note** if you like, up to 200 characters.',
          'Optional: tick **bring it from another account** (for an add) or **send it to another account** (for a take out) and choose the account. A real transfer is made too. It is shown when you have another account in the same currency. The money that is brought counts as free for the add.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'meowNY says so when an add brings a stash to its target. Without the tick, no money moves: it was in the account already, and the stash only marks it.' },
      { t: 'h3', id: 'stash-entry', text: 'Change or delete an entry' },
      {
        t: 'ol',
        items: [
          'Open the stash and click the line in its **history** (on the phone, tap it, then **edit entry**).',
          'Change the amount, the date or the note. An add stays an add, and a take stays a take: to turn one into the other, delete it and make the other.',
          'Press **done**. To remove the entry, press **delete** (at the left of the buttons) and confirm.',
        ],
      },
      { t: 'p', text: 'An entry that came with a transfer says so, and **the transfer is not changed**: it is a transfer of its own, which you edit on the **transfers** tab or in the statement of its accounts ([transfers](#transfers)). The line in the history opens it. When you delete such an entry, the question has **delete the transfer too**. Tick it to remove the transfer as well; without it, the transfer stays and only the entry goes. Deleting a transfer on its own never deletes the entry.' },
      { t: 'h3', id: 'stash-free', text: 'What is free' },
      { t: 'p', text: 'The **free** money of an account is its balance minus what its stashes hold. Archived stashes count too. An account with 1000.00 and two stashes that hold 300.00 and 200.00 has 500.00 free. Three limits keep the stashes honest:' },
      {
        t: 'ul',
        items: [
          'An add must fit in what is free. A second stash cannot set aside what the first one holds.',
          'An add must not take the stash past its target. A stash with no target is limited by what is free alone.',
          'A take cannot take out more than is saved.',
        ],
      },
      { t: 'p', text: 'The same limits apply when you change an entry or delete one. Deleting an add whose money was taken out since is not possible, because the stash would hold less than nothing. A stash cannot lower its target below what is saved.' },
      { t: 'p', text: 'You can still spend money that was set aside: the balance falls, and free can go below 0. The account says **the stashes hold more than the account has**. Nothing is taken from the stashes, but an add needs free money again, or a transfer that brings it.' },
      { t: 'h3', id: 'stash-target', text: 'The target' },
      {
        t: 'ul',
        items: [
          'A stash\'s page has a bar over its facts: the **target**, the **progress** (**37%**) and what is **left** (**750.00**). Its row in the list shows the bar with **37%**, and the target in a wide window. The small cards on an account\'s page and on the dashboard say **450.00 of 1200.00 · 37%**.',
          'With a target date, it also shows how much to save **a month**: what is left, spread over the months from now to the target date, rounded up to the cent. Saving **107.15 a month** reaches the target in time. In the month of the target date, it asks for all that is left.',
          'A stash whose target date has passed, and has not reached its target, says **overdue**, in red, and has no amount a month.',
          'At or above its target a stash says **reached**. It is never overdue.',
        ],
      },
      { t: 'h3', id: 'stash-archive', text: 'Archive, restore, delete' },
      {
        t: 'ul',
        items: [
          'For a stash you no longer add to, choose **archive** in the **⋯** menu of its page and confirm (on the phone, the open row of the list has an archive button too). It leaves the list. **What it still holds stays set aside**, and you can still take money out of it, so you can empty it. Nothing can be added until you **restore** it.',
          'Open the **⋯** menu of the list and switch on **show archived** (it says how many there are) to see the archived stashes, under their own heading. **restore** brings one back, unless another stash has taken its name.',
          'Choose **delete** in the **⋯** menu of a stash\'s page, or press the bin on an archived row, to remove it with its history. The question says what happens: the money stays in the account, what the stash held is free again, and **transfers that came with entries stay**.',
          'When the account of a stash is archived, money can only be taken out of the stash. To add again, move the stash to an account that is not archived with **edit**.',
        ],
      },
      { t: 'p', text: '**An account with a stash cannot be deleted.** It shows **archive** instead of the bin. To delete the account, move its stashes to another account first (**edit**, then **account**) or delete them.' },
      { t: 'h3', id: 'deposit', text: 'Term deposits' },
      { t: 'p', text: 'A deposit is money you fix at the bank for a term, at a rate: it **runs** until its **maturity**, then has **matured**. When the bank pays the return you **book** it, and the deposit is **booked**. Taking the money out before the maturity **breaks** it, for good. The money stays in its account, as with any stash.' },
      {
        t: 'ol',
        items: [
          'Open **stashes**, press **new stash** (on the phone, in the round **add** button\'s menu) and switch to **deposit**.',
          'Type a name and choose the account. Type the **interest rate** and its **period** (per month or per year).',
          'Check the **start date** (today) and the **maturity** (a year later). The maturity must be after the start. A target is optional.',
          'Press **done**, then **+** on its row to put the money in. A deposit takes money only while it runs.',
        ],
      },
      { t: 'p', text: 'Its page shows the rate, the start, the maturity, the **days left**, the **expected return** and the **value at maturity** (simple interest on what it holds, for the whole term) and its **state**.' },
      {
        t: 'ul',
        items: [
          '**Book the return:** at maturity the row has **return** in the place of **+**. Press it, check the amount (the expected return to start with), choose the **income category** and the day the bank paid it, and press **book return**. meowNY makes the income in the account and adds it to the deposit in one step.',
          '**Break it:** while it runs, **−** is **break deposit**. The amount is all it holds and cannot be changed: this ends the deposit for good. It then takes no more money, and its rate and dates can no longer change; only a note can.',
          '**Roll it over:** for a deposit that matured or was booked, press **edit** and give it a new maturity after today. It runs again from the old maturity.',
          '**Close it:** once it is booked or broken and empty, **archive** on its page puts it away.',
        ],
      },
      { t: 'p', text: 'A deposit is never **overdue**. Nothing is booked by itself: meowNY only works out what to expect.' },
      { t: 'h3', id: 'stash-sync', text: 'Two devices' },
      { t: 'p', text: 'The limits are checked on the device where you make the entry. After a sync, the two devices may together go past one: both added what was free, or both brought the stash to its target. Nothing is lost. The screens show it as it is: a stash can hold more than its target, and an account can have free money below 0.' },
    ],
  },
  {
    id: 'documents',
    title: 'Documents',
    blocks: [
      { t: 'p', text: 'Attach receipts and invoices to transactions, recurring items and debt entries. Each file is encrypted, up to 25 MiB. The same file added twice is stored once. Its kind is read from its first bytes, not its name.' },
      { t: 'h3', id: 'add-document', text: 'Add a document' },
      {
        t: 'ol',
        items: [
          'Open **documents**.',
          'Press **add file**, or drag files onto the window. On the phone, **add file** is a line of the round **add** button\'s menu, and **take photo** is in the **⋯** menu.',
          'Click a row (tap it on the phone) to see its details: its type, its size, where its file is, and what it is attached to. The pencil changes its name and date.',
        ],
      },
      { t: 'h3', id: 'attach', text: 'Attach one to something' },
      {
        t: 'ol',
        items: [
          'Open the dialog of a transaction, a recurring item or a debt entry (a new one, or **edit**).',
          'In a transaction, tick **attach documents**. The other two show the area already.',
          'Press **add file** (or **take photo**), or **attach** to pick a document that is in your library already.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'The documents show in the details of the item. On **documents**, a paper clip on the row of a document that is attached to something opens the list of what it is attached to.' },
      { t: 'h3', id: 'view', text: 'Look inside a document' },
      {
        t: 'ul',
        items: [
          '**Preview** (the eye on a row) shows images, PDF, text, CSV, Word (.docx) and Excel (.xlsx) inside the app. Nothing in the file runs, and nothing is written to disk.',
          '**Save a copy** writes a plain copy where you choose. It is not encrypted.',
        ],
      },
      { t: 'p', text: '**Open it in another program:**' },
      {
        t: 'ol',
        items: [
          'On **documents**, press the pencil on the row.',
          'Press **open**, and read the warning.',
          'Press **open** again. A plain copy goes to your system viewer. It is removed when the app locks or closes, and at the next start.',
        ],
      },
      { t: 'p', text: 'Unknown kinds of file can only be saved as a copy.' },
      { t: 'h3', id: 'phone-copy', text: 'Keep a copy on your phone' },
      { t: 'p', text: 'The computer is the archive. A document goes to the phone only if you ask for it.' },
      {
        t: 'ol',
        items: [
          'On the computer, open **documents**.',
          'Open the details of each document you want on the phone (click its row) and tick **keep on phone**. (Its dialog has the same switch: **keep a copy on my phone**.)',
          'Sync the phone (see [sync](#syncing)). The files come with that sync.',
        ],
      },
      { t: 'p', text: 'On the phone, a document that stays on the computer cannot be previewed: its open row says so (**location**), and **keep on phone** asks for it at the next sync. A document added on the phone moves to the computer at the next sync.' },
      { t: 'p', text: 'Backups can include the files. Exports list documents by name, never the files.' },
      { t: 'note', kind: 'info', text: 'Do not edit the `documents/` folder by hand: a file no document names is removed at the next unlock.' },
    ],
  },
  {
    id: 'debts',
    title: 'Debts and shared bills',
    blocks: [
      { t: 'h3', id: 'friends', text: 'Friends and the history' },
      { t: 'p', text: 'A friend is just a name: no phone, email or contacts. A ledger per friend, outside your budget. Green means the friend owes you, red means you owe them.' },
      { t: 'p', text: '**debts** has two tabs: **friends**, described here, and **debts**, for what you owe a bank ([loans and mortgages](#loans)). The friends tab shows your friends, each with who owes whom, the day of a reminder when there is one and the **balance**. Under them is the **history**: every change of every friend\'s debt, newest first, with the friend, what it was, the **date**, the **change** and the **balance** after it, a page at a time (**older**, **newer**). Click a friend to see only their lines, with their name and balance on top. Click them again, or press **back**, to see everyone\'s. On the phone, tap a friend, then the **history** button.' },
      { t: 'p', text: '**Add a friend:**' },
      { t: 'ol', items: ['Open **debts**, open the **⋯** menu and choose **add friend**.', 'Type a name.', 'Press **done**.'] },
      { t: 'p', text: 'The pencil on a friend\'s row renames them. The bin **remove**s them from the list: their balance and history stay, and **show removed friends**, in the **⋯** menu, lists them again: **bring back** (the button in the place of the check) puts one back.' },
      { t: 'p', text: '**Lend or borrow:**' },
      {
        t: 'ol',
        items: [
          'Open **debts** and press **lend** (money you gave, or press `n`; on the phone, a line of the round **add** button\'s menu) or **borrow** (money you got; on the phone it is in the **⋯** menu). If a friend is open, the entry starts with them.',
          'Choose the friend (or **add friend**), and type the amount and the date. Add a note if you like; it can run over several lines.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'With more than one account, the dialog also asks for the **account**: choose where the money left or arrived, so that cash you lent comes out of the cash account. The list holds the accounts in your currency only.' },
      { t: 'h3', id: 'settle', text: 'Mark as paid' },
      {
        t: 'ol',
        items: [
          'Press the check (**mark as paid**) on the friend\'s row, or **mark as paid** on their page. It is there while the balance is not zero.',
          'Check the amount. It starts as the whole balance; type less for a part payment.',
          'Set the date, and add a note if you like. With more than one account, choose the **account** the money went to or came from.',
          'Optional: choose **link a transaction**, search, and pick the one this payment was for (a bill you paid for them, say). It is only a reference: no amount changes. The history line gets a **go to the transaction** button.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'If you type more than the debt, the dialog says so first: the rest counts as a debt the other way round. Tick **no need to pay back** the difference to drop it, and the whole balance is recorded.' },
      { t: 'h3', id: 'loans', text: 'Loans and mortgages' },
      { t: 'p', text: 'A **credit**, a **loan** or a **mortgage** is an account you owe on: its balance is what you owe, below 0. You pay it from one of your accounts, and part of each payment is interest. They are on the **debts** tab of **debts**, not on **accounts**, with **total owed** above them.' },
      {
        t: 'ol',
        items: [
          'Open **debts**, choose the **debts** tab and press **new debt** (on the phone, in the round **add** button\'s menu).',
          'Type a name, and choose **credit**, **loan** or **mortgage**.',
          'Type what is **owed** (without a minus) and since when.',
          'Optional: type the **interest rate** and its **period**, and the **installment** you pay each time.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Each debt shows what is **owed**, the **installment** and the **payoff**: the month it is paid off if you keep paying the installment, or **never** when the installment does not even cover the interest. Open its row (the chevron, or a tap on the phone) for the **interest rate** and the **interest per month**. The estimate is simple interest, a month at a time.' },
      { t: 'p', text: '**Pay one:**' },
      {
        t: 'ol',
        items: [
          'Press **pay** on its row, or on its page (on the phone, in the round **add** button\'s menu).',
          'Choose the account to **pay from**. The amount starts as the installment, and the **interest** as the interest of a month on what is owed.',
          'Check both against your bank. With interest, choose the **interest category** (an expense category). The dialog shows the **principal** (the payment minus the interest) and what is **owed after**.',
          'Press **done**. meowNY makes a transfer of the principal into the debt, and an expense of the interest in the account you paid from.',
        ],
      },
      { t: 'p', text: 'The two are ordinary rows afterwards: you find the transfer on the **transfers** tab and the interest among your expenses. A debt that is paid off says **paid off**; one you paid too much on, **overpaid**. Its page is an account\'s page ([look at one account](#look-account)), with owed, rate, installment and payoff, and **pay**. A debt holds no stash.' },
      { t: 'h3', id: 'split', text: 'Split a bill' },
      {
        t: 'ol',
        items: [
          'Choose **split a bill** in the **⋯** menu of **debts** (a friend you have open is on it already), or add a transaction and tick **split this bill**. Only an expense can be split. On the phone, add an expense and tick it there.',
          'Choose **how to split**: **equally**, **by percent**, **by shares** or **by amounts**.',
          'Add the friends who share it: choose one under **add friend**, or type a new name (it becomes a friend).',
          'For percent or amounts, type each part. Leave one field empty (yours or a friend\'s) and it gets what is left. With two or more empty, the dialog shows **left to assign**. For shares, type a whole number for each: 2, 1, 1.',
          'If a friend paid the bill, choose them under **paid by**. It shows once a friend is on the bill.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Budgets, statistics and exports count only your part; each friend\'s part is added to what they owe. If a friend paid, you owe them your part (it must be above 0), and the other friends owe them, not you. To change any of it, edit the bill. A recurring item can be split too, with one person who pays: each payment copies the split.' },
      { t: 'h3', id: 'bill-code', text: 'Send a friend their part' },
      {
        t: 'ol',
        items: [
          'On **transactions**, click the bill to open it. Press **code** next to the friend\'s part. The friend who paid has no code. The same button is on a friend\'s history line and on a recurring item.',
          'If meowNY does not know your name yet, it asks which name your friend should see. **settings › name for friends** changes it.',
          'Show the QR code, or press **copy** and send the text. It holds that friend\'s part only, nobody else\'s.',
        ],
      },
      { t: 'h3', id: 'bill-take', text: 'Take in a bill code' },
      {
        t: 'ol',
        items: [
          'Open **debts** and open the **⋯** menu. Choose **paste a code**, or on the phone **scan** and point the camera at the code. (The round **add** button of the phone has **scan a bill** too.)',
          'Paste the code and press **read the code**.',
          'Check who it is from, what it is for and your part. If the bill is in another currency, type the rate.',
          'Choose which friend it belongs to, or **new friend**. When a friend paid the bill, you owe that friend.',
          'To count your part as an expense, tick **add my part to my expenses** and choose a category.',
          'Press **accept**, or **deny**. Nothing is written before you accept.',
        ],
      },
      { t: 'h3', id: 'debt-notifications', text: 'Debt notifications' },
      { t: 'p', text: 'A reminder comes every week, 2 weeks or month after a debt last changed. You choose how often and when, per device. The text holds a count, never a name or an amount.' },
      {
        t: 'ol',
        items: [
          'Open **debts**, open the **⋯** menu and choose **notifications**. The number is how many reminders are waiting.',
          'Each friend with a reminder is a line. Press the name to open their history, press the x to clear that one, or press **clear all**.',
          'For the time and how often (**never**, every week, every 2 weeks, every month), open the **settings** tab there.',
        ],
      },
      { t: 'p', text: 'Clearing only hides a reminder on this device. The same friend\'s next reminder comes back.' },
      { t: 'h3', id: 'statement', text: 'Save a statement' },
      {
        t: 'ol',
        items: [
          'On the computer, open **debts** and click a friend.',
          'Under **statement**, choose the **file type**: TXT, CSV or PDF.',
          'Press **save statement**, look at the preview, and save it where you choose. The file is not encrypted.',
        ],
      },
      { t: 'p', text: 'Recurring payments past their day are not debts here: pay or skip them under **recurring › due payments**.' },
    ],
  },
  {
    id: 'phone',
    title: 'Your phone',
    blocks: [
      { t: 'p', text: 'The phone app (Android; iPhone is not available yet) keeps a full copy and works offline. It syncs over your home wifi, with no server.' },
      { t: 'h3', id: 'pairing', text: 'Pair once' },
      {
        t: 'ol',
        items: [
          'On the phone, open meowNY and press **get started**. Choose a language, type a passphrase twice (at least 12 characters, or a few random words) and press **create**.',
          'On the computer, open **settings › phone** and press **add a phone**. A QR code appears, with a countdown.',
          'On the phone, press **scan the code** and point the camera at it.',
          'The computer shows six digits. Type them on the phone.',
          'Wait for **paired with** and the name of the other device. Your data is copied to the phone.',
        ],
      },
      { t: 'h3', id: 'syncing', text: 'Sync' },
      {
        t: 'ol',
        items: [
          'On the computer, open **settings › phone**. Choose a **direction**: **both ways**, **phone to computer** or **computer to phone**.',
          'Press **sync with phone**. A QR code appears.',
          'On the phone, open the menu (the logo at the left of the title), choose **sync** and press **scan the code**.',
          'If the same thing changed on both devices, the conflicts are shown side by side. Choose which version to keep, one by one or **all from** one device.',
          'A summary tells what was added, changed or deleted.',
        ],
      },
      {
        t: 'ul',
        items: [
          'If the clocks of the two devices differ, the sync still runs and both say so afterwards. The dates you typed stay as they are; the computer\'s time decides which change is newer and gives the phone its dates and reminders.',
          'Both devices must be on the same network. A network that keeps devices apart blocks it: use the phone\'s hotspot. If the phone still cannot find the computer, open **another address…** under the direction and type the computer\'s address on your home network, like `192.168.1.20`.',
          'iPhone: allow **local network** when asked, then scan again. Documents are previewed in the app; iPhone has no "open".',
          'A passphrase locks the phone. Five wrong tries erase it; pair again to get the data back.',
        ],
      },
      { t: 'h3', id: 'lost-phone', text: 'A lost phone' },
      { t: 'ol', items: ['On the computer, open **settings › phone**.', 'Press **remove** beside the phone, then **yes, remove**.'] },
      { t: 'p', text: 'A removed phone has to be paired again.' },
      { t: 'h3', id: 'on-phone', text: 'On the phone' },
      {
        t: 'ul',
        items: [
          'The logo at the left of a page\'s title opens the menu: every page (dashboard, transactions, accounts, stashes, categories, recurring, debts, documents, sync and settings) and **lock**. A swipe to the right from the left edge of the screen opens it too. Choosing a page, a tap beside the menu or a swipe to the left closes it.',
          'A page has a **⋯** button at the top for its second actions and for switches such as **show archived**. A **?** beside a title explains the page.',
          'The round **add** button (on every page but settings and sync) has the page\'s main action first (**add account**, **new stash**, **add recurring transaction**, **lend**, **add file** and the like), then **add transaction**, **add category**, **add friend** and **scan a bill**. A page whose main action is one of those does not repeat it.',
          'Tap a row to open it: it shows its facts, one to a line, and its buttons (edit, delete and the like). Documents: **take photo** (in the **⋯** menu), and **keep on phone** for a file that is on the computer.',
          'Export, backups, the recovery key, pairing and the command line are on the computer only.',
          'Debt reminders ring on the phone at the time you chose, also when the app is closed.',
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
          'Open **settings › backup**.',
          'Under **manual backup**, type a backup passphrase twice. Tick **include the documents** if you want the files too.',
          'Press **back up now**, and choose where to save the file.',
        ],
      },
      { t: 'h3', id: 'schedule', text: 'Back up on a schedule' },
      {
        t: 'ol',
        items: [
          'Open **settings › backup**. Under **scheduled backups**, type a backup passphrase twice.',
          'Choose **how often**: every day, week or month.',
          'Press **choose folder and start**. meowNY keeps the newest 10 backups there. **stop scheduled backups** turns it off.',
        ],
      },
      { t: 'h3', id: 'restore', text: 'Restore a backup' },
      {
        t: 'ol',
        items: [
          'On a new install, press **restore from a backup** on the first screen. Or, in an app that has data, open **settings › backup** and go to **restore**.',
          'Type the backup passphrase. In **settings**, also type RESTORE to confirm. On a new install, add a passphrase for the app too, or leave it empty.',
          'Press **restore from backup** (on a new install **choose backup file and restore**) and choose the file.',
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
      { t: 'p', text: 'Accounts and transfers are in the files:' },
      {
        t: 'ul',
        items: [
          'Each transaction, recurring item and debt entry names its account.',
          'A JSON or Excel export with no limit also lists the accounts with their balances and details (BIC, group, notes, virtual balance, whether net worth counts them, currency and rate), and the transfers. A transaction in an account in another currency has its amount in that currency too, and a transfer between currencies what arrived.',
          'A limited export names only the accounts of what it holds, with no balances, numbers or transfers.',
          'Backups hold accounts and transfers like everything else.',
        ],
      },
      { t: 'p', text: 'Stashes are in the files too:' },
      {
        t: 'ul',
        items: [
          'A JSON or Excel export with no limit lists the stashes with their account, target, target date, notes, whether they are archived, what each holds and every entry. Excel has two sheets for them, **Stashes** and **Stash entries**; JSON also says how much each account has set aside.',
          'A limited export has no stashes, and no set aside amounts. The PDF report of every category adds a short table of the stashes, with what each held on the last day of the period.',
          'Backups hold stashes like everything else.',
        ],
      },
      { t: 'p', text: '**Delete all my data** (settings › your data) wipes the database, documents, keys and settings on this device. You type DELETE to confirm.' },
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
          'The database is encrypted (SQLCipher). Its key is kept only as two wrapped copies: one opened by the keychain (and your passphrase), one by the recovery key.',
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
          ['Categories, budgets, accounts (names, numbers, BIC, groups, notes, opening and virtual balances, currencies and rates, whether net worth counts them), transfers, stashes (names, targets, target dates, notes) and their entries (amounts, dates, notes), transactions (with the amount in the account\'s currency), recurring items, debts, the change history', '`meowny.db`', 'Encrypted'],
          ['Documents', '`documents/`, one sealed file each with a random name', 'Encrypted, per-file key'],
          ['Lock settings, the wrapped keys', '`vault.json`', 'No financial data'],
          ['Language, theme, text size, sidebar, this device\'s name, the name on bill codes, the time and how often of debt reminders', 'The window\'s local storage', 'No financial data, not encrypted'],
          ['The filters you chose (kind of period, category, rows per page, document type, removed friends), the account filter, whether archived accounts or stashes are shown; the last account used (a number); the last category used for a bill code; the day a notice or reminder was shown; the reminders you cleared (friend numbers and days)', 'The window\'s local storage', 'Numbers, days and choices only: no amounts, descriptions or names. The filters are forgotten when all data is erased or restored'],
          ['A document open in another program', '`open/`, only while open', '**Not encrypted**, removed on lock and close'],
          ['Command line tokens', '`meowny.db` (only a hash); the token file of `meowny login`', 'Read-only by default, expire, revocable'],
        ],
      },
      { t: 'p', text: 'The month or the dates a page was showing are kept in memory only while the app is open. They are never written down.' },
      { t: 'h3', id: 'leaves', text: 'What leaves the device' },
      {
        t: 'table',
        head: ['What', 'When'],
        rows: [
          ['Phone sync', 'When you start it. Straight to the paired phone on your home network, sealed with keys from the pairing. No relay or server'],
          ['Files: backups, exports, statements, copies of documents', 'Where you save them. Only backups are encrypted'],
          ['A bill code', 'When you show or copy it: one friend\'s part of one bill, not encrypted'],
          ['Notifications', 'Due items by name on the computer; debt reminders by count only'],
          ['The clipboard', 'The recovery key or a new token, only when you press copy; a bill code, when you press copy'],
          ['A web page', 'When you press **check for updates** or another link in the footer: your browser opens it, and meowNY sends nothing'],
        ],
      },
      { t: 'p', text: 'There are no other network calls. CI lists every place in the code that could open a connection and fails on a new one. The command line API listens on `127.0.0.1` only, is off until you turn it on, and needs a token; exports, backups and copies of documents through it need your yes in the app each time.' },
      { t: 'p', text: '**Delete all my data** removes the database, `documents/`, `open/`, `vault.json` and the keychain entry. It leaves backups and exports you saved elsewhere, and the token file of `meowny login` (which stops working).' },
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
          ['arrow keys', 'the next button, row or field that way; in a text field, left and right move the cursor first'],
          ['`j` `k`', 'next, previous row'],
          ['`Alt` + up or down arrow', 'move a category, an account or a stash one place'],
          ['`Alt+1` `Alt+2` `Alt+3`', 'in the add dialog: the expense, income, transfer tab (`Option` on a Mac; the recurring dialog has the first two; the stash dialog has add and take out)'],
          ['`Esc`', 'close a row, a list or a dialog'],
          ['`Ctrl/Cmd+L`', 'lock'],
          ['`Ctrl/Cmd+B`', 'sidebar'],
        ],
      },
      { t: 'p', text: 'A name that is cut off with "…" slides into view while you point at it.' },
    ],
  },
  {
    id: 'cli',
    title: 'Command line',
    blocks: [
      { t: 'p', text: '`meowny` lets scripts and AI agents use the running desktop app. The Linux `.deb` installs it; the release also has it on its own for Linux and Windows.' },
      { t: 'ol', items: ['Get `meowny`: the `.deb` installs it; otherwise see [set it up](cli.html#quick-start).', 'In the app: **settings › CLI access**. Turn it on and make a token (it is shown once).', 'Run `meowny login`, paste the token and press Enter.', 'Check: `meowny status`.'] },
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
