// The guide: what each part of meowNY does, and what it keeps where. Every how-to is a numbered list of steps; what a thing
// is, and what it stores, is a short paragraph or list. This page is the user documentation; there is no Markdown copy of it.
// How sync, documents and debts are built is in the design notes in docs/.

import { REPO, SPONSOR } from './render.mjs'
import { gallery } from './shots.mjs'

const doc = (name) => `${REPO}/blob/main/docs/${name}`
const RELEASES = `${REPO}/releases/latest`

/** The screenshots each section shows, under its text; missing files are left out. */
const PICTURES = {
  start: ['lock', 'phone-lock'],
  budgeting: ['dashboard', 'categories', 'recurring', 'transactions'],
  documents: ['documents', 'preview-pdf', 'preview-csv'],
  debts: ['debts', 'friend', 'split', 'bill-code', 'debt-history'],
  phone: ['phone-sync', 'phone-dashboard', 'phone-quick-add', 'phone-debts', 'phone-categories'],
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
      { t: 'p', text: 'The pages you use most: **dashboard**, **transactions**, **categories** (the budgets are there too) and **recurring**. On the phone they are tabs at the bottom, and the round **add** button replaces **add transaction** and **add category**.' },
      { t: 'h3', id: 'dashboard', text: 'Dashboard' },
      {
        t: 'ul',
        items: [
          'Income, expenses, savings and the savings rate of a month, with how much each changed from the month before. Click a month on the year chart to look at it; the arrows change the year, and arrow keys move along the chart.',
          'Spending by category, your budgets against what you spent, what friends owe you and what you owe, and the recurring items that are overdue, due soon or split with friends.',
          'A new, empty dashboard shows **first steps**: add a transaction, set a budget, add a friend, pair your phone.',
        ],
      },
      { t: 'h3', id: 'transactions', text: 'Transactions' },
      { t: 'p', text: 'Spending is red with a minus, income is green with a plus. A split bill shows only your part.' },
      {
        t: 'ol',
        items: [
          'Open **transactions** and press **add transaction** (or press `n`).',
          'Type a name and an amount, and choose a category. To make a new one, choose **add category** in the list.',
          'Check the payment date. It starts at today. Day, month and year are lists you can also type into: for a month, `nov`, `november` or `11` all work. Press Enter or leave the field to take it. A date that cannot exist stays red.',
          'Add a description if you like. It can run over several lines.',
          'Optional: tick **split this bill** to share it with friends ([split a bill](#split)), or **attach documents** to keep a receipt with it ([attach](#attach)).',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'Click a row (tap it on the phone) to open its details: how a bill is split, the description and the documents. The pencil edits a transaction and the bin deletes it.' },
      { t: 'h3', id: 'find', text: 'Find transactions' },
      {
        t: 'ol',
        items: [
          'Open **transactions**. Under **period** choose **month**, **year** or **range**.',
          'For a month or a year, step with the arrows (or `[` and `]`). **current** brings you back to this month or year. For a **range**, choose the **from** and **to** days.',
          'Choose a **category**, or type in **search** (press `/`) part of a name or description.',
          'Under the list, choose how many **rows** a page shows (10, 50 or 100) and use the page buttons.',
        ],
      },
      { t: 'p', text: 'meowNY remembers the kind of period, the category and the number of rows the next time. The dates are remembered only until you close the app. On the phone, the period and the category are behind the **filters** button.' },
      { t: 'h3', id: 'categories', text: 'Categories' },
      { t: 'p', text: 'Categories sort transactions into expenses and income. **categories** shows both side by side for one month (change it with the arrows or `[` and `]`), and each category shows what you spent or earned that month next to its budget. Spent is green within the budget and red above it; earned is green above its budget.' },
      {
        t: 'ol',
        items: [
          'Open **categories** and press **add category** (or press `n`).',
          'Type a name, and choose whether it is an expense or income.',
          'Type a budget from this month on, if you want one.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Change the order:**' },
      {
        t: 'ol',
        items: ['Find the grip at the start of a category row.', 'Hold it and drag the row up or down. With the keyboard, focus the row and press `Alt` with the up or down arrow.'],
      },
      { t: 'p', text: 'The order is kept for expenses and for income apart, and goes to your phone with the next sync. **Delete** asks for which months: only this month (the category stays), this month and later, or all months.' },
      { t: 'h3', id: 'budget', text: 'Budgets' },
      { t: 'p', text: 'A budget is set per month and category, and carries into later months.' },
      {
        t: 'ol',
        items: [
          'Open **categories** and choose the month.',
          'On the expenses or the income side, press the pencil on **monthly budget**.',
          'Type the amount and press **save**. To remove the budget, press **clear**.',
          'For one category, press the pencil on its row instead, type the amount under **budget from … on**, and press **done**. You can rename it there too.',
        ],
      },
      { t: 'p', text: 'Change a budget of a past month and meowNY asks how far the change reaches: only that month, that month and later, or every month. Category budgets cannot add up to more than the monthly budget. The list of months under the budgets shows each month\'s spending against its budget; click one to look at it.' },
      { t: 'h3', id: 'recurring', text: 'Recurring items' },
      { t: 'p', text: 'Rent, salary, subscriptions: what comes back. The list shows active items first, then paused and ended ones, each group by category.' },
      {
        t: 'ol',
        items: [
          'Open **recurring** and press **add recurring transaction** (or press `n`).',
          'Type a name and choose a category. Type an amount, or leave it empty for a bill with no fixed amount: it shows **varies** and asks for the amount when you pay it.',
          'Set the **first due date**, and **repeat every** a number of days, weeks, months or years.',
          'Tick **stops on a date** if it ends, and set that date.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: '**Pay one:**' },
      {
        t: 'ol',
        items: [
          'When an item is due, press **mark as paid** on its row.',
          'Check the **amount paid** (type it for one that varies) and the date it was **paid on**.',
          'Press **done**. meowNY makes a transaction and moves the item to its next date.',
        ],
      },
      { t: 'p', text: 'Open a row (click it, or tap it) for more: **skip** passes over the next payment without a transaction, **pause** stops it coming due until you **resume** it, and **end** stops it for good. A split item has a **code** for each friend\'s next payment.' },
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
    id: 'documents',
    title: 'Documents',
    blocks: [
      { t: 'p', text: 'Attach receipts and invoices to transactions, recurring items and debt entries. Each file is encrypted, up to 25 MiB. The same file added twice is stored once. Its kind is read from its first bytes, not its name.' },
      { t: 'h3', id: 'add-document', text: 'Add a document' },
      {
        t: 'ol',
        items: [
          'Open **documents**.',
          'Press **add file**, or drag files onto the window. On the phone you can also press **take photo**.',
          'Click a row (tap it on the phone) to see its details. The pencil changes its name and date.',
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
      { t: 'p', text: 'The documents show in the details of the item. On **documents**, the **links** column shows how many things a document is attached to.' },
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
          'Tick the box in the **phone** column of each document you want on the phone. (Its dialog has the same switch: **keep a copy on my phone**.)',
          'Sync the phone (see [sync](#syncing)). The files come with that sync.',
        ],
      },
      { t: 'p', text: 'On the phone, a document that stays on the computer cannot be previewed: its row says so, and **keep on phone** asks for it at the next sync. A document added on the phone moves to the computer at the next sync.' },
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
      { t: 'p', text: '**debts** shows your friends on the left. On the right is the **history**: every change of every friend\'s debt, newest first, a page at a time (**older**, **newer**). Click a friend to see only their lines, with their name and balance on top. Click them again, or press **back**, to see everyone\'s. On the phone, tap a friend, then **history**.' },
      { t: 'p', text: '**Add a friend:**' },
      { t: 'ol', items: ['Open **debts** and press **add friend**.', 'Type a name.', 'Press **done**.'] },
      { t: 'p', text: 'The pencil on a friend\'s row renames them. The bin **remove**s them from the list: their balance and history stay, and **show removed friends** lets you **bring back** one.' },
      { t: 'p', text: '**Lend or borrow:**' },
      {
        t: 'ol',
        items: [
          'Open **debts** and press **lend** (money you gave, or press `n`) or **borrow** (money you got). If a friend is open, the entry starts with them.',
          'Choose the friend (or **add friend**), and type the amount and the date. Add a note if you like; it can run over several lines.',
          'Press **done**.',
        ],
      },
      { t: 'h3', id: 'settle', text: 'Mark as paid' },
      {
        t: 'ol',
        items: [
          'Press **mark as paid** on the friend\'s row, or on their page. It is there while the balance is not zero.',
          'Check the amount. It starts as the whole balance; type less for a part payment.',
          'Set the date, and add a note if you like.',
          'Optional: choose **link a transaction**, search, and pick the one this payment was for (a bill you paid for them, say). It is only a reference: no amount changes. The history line gets a **go to the transaction** button.',
          'Press **done**.',
        ],
      },
      { t: 'p', text: 'If you type more than the debt, the dialog says so first: the rest counts as a debt the other way round. Tick **no need to pay back** the difference to drop it, and the whole balance is recorded.' },
      { t: 'h3', id: 'split', text: 'Split a bill' },
      {
        t: 'ol',
        items: [
          'Press **split a bill** on **debts** (a friend you have open is on it already), or add a transaction and tick **split this bill**. Only an expense can be split. On the phone, add an expense and tick it there.',
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
          'Open **debts**. Press **paste a code**, or on the phone **scan** and point the camera at the code.',
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
          'Open **debts** and press **notifications**. The number is how many reminders are waiting.',
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
          'On the phone, open **sync** (the arrows in the top bar) and press **scan the code**.',
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
          'Dashboard, transactions, recurring, debts and categories are tabs at the bottom. Documents, sync, settings and lock are the icons in the top bar.',
          'The round **add** button adds a transaction, a category or a friend, or scans a bill code.',
          'Tap a row to open it. Documents: **take photo**, and **keep on phone** for a file that is on the computer.',
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
          ['Categories, budgets, transactions, recurring items, debts, the change history', '`meowny.db`', 'Encrypted'],
          ['Documents', '`documents/`, one sealed file each with a random name', 'Encrypted, per-file key'],
          ['Lock settings, the wrapped keys', '`vault.json`', 'No financial data'],
          ['Language, theme, text size, sidebar, this device\'s name, the name on bill codes, the time and how often of debt reminders', 'The window\'s local storage', 'No financial data, not encrypted'],
          ['The filters you chose (kind of period, category, rows per page, document type, removed friends); the last category used for a bill code; the day a notice or reminder was shown; the reminders you cleared (friend numbers and days)', 'The window\'s local storage', 'Numbers, days and choices only: no amounts, descriptions or names. The filters are forgotten when all data is erased or restored'],
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
      { t: 'p', text: `How it is built: [sync](${doc('sync-design.md')}), [documents](${doc('documents-design.md')}), [debts](${doc('debts-design.md')}).` },
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
          ['`g` then `d` `t` `c` `r`', 'dashboard, transactions, categories, recurring'],
          ['`g` then `f` `o` `e` `s`', 'debts, documents, export, settings'],
          ['`n`', 'new'],
          ['`/`', 'search'],
          ['`[` `]`', 'previous, next month (or year)'],
          ['`t`', 'this month (or year)'],
          ['arrow keys', 'the next button, row or field that way; in a text field, left and right move the cursor first'],
          ['`j` `k`', 'next, previous row'],
          ['`Alt` + up or down arrow', 'move a category one place'],
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
