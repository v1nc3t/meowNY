# Debts and shared bills: a guide

meowNY keeps track of who owes whom, and of bills you share. Everything is in your own ledger, on your own devices: nothing is uploaded, nobody else needs the app, and nothing is sent to a friend unless **you** show or send it.

This is the guide for using it. How it is built is in [debts-design.md](debts-design.md), and exactly what is stored where is in [data-inventory.md](data-inventory.md).

## Friends

A **friend** is a name and nothing else: no phone number, no email, no address, and nothing is read from your contacts. Add one on the **friends** tab of the **debts** page (**add friend**), or when you need one (on a bill, in an entry, when you take in a code). On the **friends** tab each friend has **edit** (the name, in a dialog), **delete** and **mark as paid**. A friend is never erased: **delete** takes them off the list (after asking), and **show removed friends** brings them back into view, with **bring back** on their row; their balance and history stay, and the name can be used again.

A name typed on a bill becomes a friend at once. (A **guest**, a name on one bill who is not a friend and makes no debt, can still be made from the command line; a bill that has one opens and saves as before.)

## What you owe and are owed

On a friend's page, the **history** lists every line by date with the balance after it. A positive balance is what they owe you, a negative one what you owe them.

- **lend** and **borrow** (buttons at the top of the **debts** page and on a friend's page) make an entry. **Paid back to me** and **paid back by me** are the other two kinds (pick them in the entry's **kind**). An entry has a date, an amount, a note, and can carry documents (a loan note, an invoice), attached like a transaction's.
- **Mark as paid** takes the whole balance, or an amount you type. An amount **larger than the debt** is accepted: the dialog says first that the balance will turn around (you then owe them the rest). When the balance reaches zero, the friend's reminder is cleared.
- **borrow** can also tick **this was my share of a bill: add it to my expenses**. Someone else paid and you owe them: your share is an expense in the month, and what you owe is the debt. Paying them back later is an entry and never a second expense.
- None of this is income or expense, and none of it touches a budget. A loan does not make your income higher, and a repayment does not hide a real cost.

The totals (**owed to you**, **you owe**, **net**) are on a card of the **dashboard**; the **debts** page lists the friends, the payments that are overdue and the history.

The **debts** page has three tabs:

- **friends**: **add friend**, then a row for each friend with what is owed (green when they owe you, red when you owe them), a reminder that has come, and the buttons **edit**, **delete** and **mark as paid**. The name leads to the friend's page (the lines of the ledger, and **save statement**). The buttons at the top of the page are **lend**, **borrow**, **split a bill** and **paste a code** (**scan a bill** on a phone).
- **due payments**: the recurring payments that are **overdue** (their day has gone by: one that falls due today is not here until tomorrow, though the recurring page shows it in red from its day) and are not paid or skipped, **one row for each occurrence** (a rent that was not paid for three months is three rows), the oldest first, with the day it fell due, what it is for (**varies** for a bill whose amount changes), how many days late it is, and a **total due**. Only payments count: a recurring income is not something you owe, and a paused or ended item is not listed. An item that was forgotten for years shows its oldest 24 occurrences. **Mark as paid** on a row opens the payment dialog of that item on the recurring page (it pays the oldest occurrence, so only the oldest row of an item has the button). Paying or skipping on the recurring page removes the row here.
- **history of debts**: every change to what you owe or are owed, newest first, 50 lines to a page: the lines of every friend (lent, borrowed, paid back, the parts of shared bills, with the balance after each), the recurring payments that were **overdue**: those that were **paid late** (on the day they were paid, with the day they fell due) and those that are **overdue now**. A payment made on its day or before it is **not** in the history, since it was never late. A friend's line leads to the friend's page, a recurring payment's to the recurring page.

None of this is stored on its own: the due payments and the history are worked out from the recurring items, the payments and the friends' ledgers, so they cannot disagree with those screens. One thing follows from that: a payment you **skipped** leaves no line in the history (it is only in the change history), and neither does a payment made before the app kept the day it fell due.

## Splitting a bill

You paid for a dinner; four of you shared it. Tick **split this bill** when you add or edit a transaction (or press **split a bill** on the debts page or a friend's page, which opens a new transaction with the split on and that friend on the bill):

1. Choose **how to split**: **equally**, **by percent** (each person's percent; together exactly 100), **by shares** (2, 1, 1) or **by amounts** (each person's amount; together exactly the bill).
2. Add people in the one field under the lines: choose a friend from the list (it narrows as you type), or type a name that is not in it and choose **add "name" as a friend**. A new name becomes a friend, and their part of the bill is what they owe you. You are always the first line.
3. Every line shows what it comes to, worked out by the app as you type. For percent and amounts, **give the rest to me** makes your own line the difference. The dialog says what is **left to assign** until the numbers add up; it cannot be saved before.

What changes:

- The transaction keeps **what you paid**. **Budgets, statistics, reports and exports count only your part**, and the list shows your part as its amount; press **details** under it to see, in a section called split, what was paid and who has which part (and, if there is one, the description in its own section). To change a split, press **edit**: the same form has the switch.
- Each friend's part is added to what they owe you.
- The odd cent goes to whoever has the largest remainder, you first; the parts always add up to the bill to the cent.
- Changing the **amount** of a shared bill scales every part, yours included. Deleting a transaction asks first (a shared bill's parts go with it, so the friends owe you less). A shared bill cannot be an income.
- A saved bill opens as exact amounts. Changing how it is split starts the numbers again.

A **recurring item** can be split too (tick **split this bill** in its form). Each payment is then split by the same proportions, whatever it was paid for. It is kept as proportions, so an item with no fixed amount can be split. The payments already made keep their parts. **Details** on the item in the list shows the proportions (you 2, Anna 1, …).

## Showing a bill to a friend: the bill code

On a shared bill, press **show code** on the bill in the list (with one friend it opens at once; with several it opens the **details**, where each friend has their own **show code**), or on a bill line in the friend's page. A **recurring item** that is split has **show code** too, for the friend's part of its **next payment** (it needs a fixed amount; the dialog says which day it is for). The name your friend sees is the one you gave when you made the account; **settings** changes it (it is kept on this device), and the app only asks for it here when there is none (for instance after restoring a backup). Show the code of a payment once, either before it is paid (the item's) or after (the bill's): the two are different codes, and a friend who takes in both owes it twice. The dialog shows a **QR code** and **copy** copies the same text, so you can send it in the chat you already use.

The code holds this bill's name and date, your name, your friend's part and the whole bill, and the currency. It holds nobody else's name or part. It is **not stored and not encrypted**: whoever sees the screen, or the text you copied, can read it. Show it only to the friend it is for. If a code is too long for a QR code (long names), the app says so and the text is the way.

## Taking a bill from a friend

On the **debts** page, **scan a bill** (phone) opens the camera, and **paste a code** (computer, and the phone) takes the text. The app reads the code and **writes nothing** until you accept. It shows who it is from, the bill, the date and your part, and asks:

- **Which friend**: the one whose name is the sender's is chosen if you have one, or **new friend** with the name filled in.
- **Add my part to my expenses** (ticked) with a category: your share becomes an expense, and the entry you owe the sender is linked to it.
- For a code in **another currency**: **how much is 1 USD in EUR?** The rate is yours to type; nothing is fetched. The converted amount is shown and you can change it. Accept waits for it.

**Accept** makes the friend (if new), the expense (if ticked) and the entry together, or nothing. **Deny** closes it and stores nothing. A code you already took in is not taken twice. If your friend corrects a bill, ask for a new code: delete the old entry, then take in the new one.

The sender is not told that you scanned it or paid. Each of you marks payments yourself.

## Reminders

Reminders are **one setting for all debts**, in **settings** under **notifications**: **remind me about each debt**, **never**, **every week**, **every 2 weeks** or **every month** (every week unless you change it). Every friend whose balance is not zero is reminded that long after the **day their debt last changed** (the latest line of their statement), and again after as long, until it is settled. A new line (money lent, a part of a bill, a repayment) starts the count again. There is nothing to set on a friend's page.

- The notification says **"meowNY: a debt reminder"** and how many friends there are to look at: **never a name or an amount**, because the app is locked when it rings and a notification can be seen on a lock screen and kept by the system. The **debts** page then names who is due.
- **On the computer**, it rings only while meowNY is open and unlocked, at the **time of day** you set in **settings** (09:00 by default), or at once if the day came while the app was closed. The **notifications** switch in settings turns these off, with the recurring items' reminders.
- **On the phone**, the app hands the system the next few days, so they ring **with the app closed**, and it does so again at each unlock, after each sync and when the setting or a debt changes. The first time it needs the notification permission and asks. A friend you settle on another device can still ring on the phone until you open the app.
- The time of day and the period are set for each device.

## Files to send

On a friend's page, **save statement** writes that friend's lines and the balance after each. Choose **TXT**, **CSV** or **PDF** (the PDF is in your language). The button first shows a **preview** of the file (the text, the CSV, or the pages of the PDF), exactly as it will be saved; nothing is written until you press **save** in the preview, and only then does the app ask where. These are plain files, **not encrypted**, written where you choose and sent to nobody; sending is up to you. The phone has no save button. (Who owes what and the whole ledger can still be written from the command line, `meowny export debts`, and a PDF report can list them.)

## Backups, the phone and scripts

- Friends, entries and parts are in **backups** and in the **exports** (all of it with no limit; with a date or category limit only the bills in it and the friends they name, by name, with no balance and no reminder).
- They **sync** with the phone like everything else. If two devices each added a friend of the same name, the two become one.
- In an export (JSON and Excel) the **due payments** and the **history of debts** are parts of their own next to the **debts** (the friends); a PDF report has three options: a **list of debts** (who owes what), a **list of due payments** and the **history of debts**.
- From a script: `meowny people|ledger|debts|splits ...`, `meowny transactions split`, `meowny recurring split`, and `meowny export debts`. A bill code and a reminder need you at the app, so they are not in the command line.

## What to know

- A friend's balance is a running sum, not matched to bills: a repayment is not tied to one bill.
- Money is in your currency. A code in another currency is converted with the rate you type, once, when you accept.
- The bill code is a notice, not a link: later edits of the bill do not follow it, and the sender is not told what you did.
- Reminders on the phone depend on the system: on some phones the maker's battery saving can delay or drop scheduled notifications.
