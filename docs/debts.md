# Debts and shared bills: a guide

Who owes whom, and bills you share, in your own ledger on your own devices. Nothing is uploaded, nobody else needs the app, and nothing reaches a friend unless you show or send it.

Built: [debts-design.md](debts-design.md). Stored where: [data-inventory.md](data-inventory.md).

## Friends

- A friend is a name and nothing else: no phone, email, address or contacts.
- Add one on the **friends** tab (**add friend**), on a bill, in an entry, or when you take in a code.
- Each friend has **edit** (name), **delete** and **mark as paid**.
- **delete** takes a friend off the list after asking; **show removed friends** and **bring back** undo it. Balance and history stay.
- A guest (a name on one bill, no debt) can be made from the command line.

## What you owe and are owed

- **lend** and **borrow** (top of the debts page and a friend's page) make an entry. The other two kinds, **paid back to me** and **paid back by me**, are in the entry's **kind**.
- An entry has a date, amount, note, and can carry documents (a loan note, an invoice).
- **Mark as paid** takes the whole balance or an amount you type. An amount over the debt is accepted; the dialog says first that the balance turns around.
- **borrow** can tick "this was my share of a bill: add it to my expenses". Paying back later is an entry, never a second expense.
- A friend's page lists every line by date with the balance after it. Positive means they owe you.
- None of it is income or expense, and none of it touches a budget.
- Totals (**owed to you**, **you owe**, **net**) are on the dashboard.

The **debts** page has three tabs:

| Tab | Shows |
|---|---|
| friends | each friend with what is owed (green: they owe you; red: you owe them), a reminder that has come, and the buttons. Page buttons: **lend**, **borrow**, **split a bill**, **paste a code** (**scan a bill** on a phone) |
| due payments | recurring payments past their day and not paid or skipped, one row per missed occurrence, oldest first, with day, purpose (**varies** for a changing amount), days late and **total due**. A payment due today appears tomorrow. At most 24 per item. **Mark as paid** opens the item's pay dialog (oldest occurrence only) |
| history of debts | newest first, 50 per page: every friend's lines with balance, recurring payments **paid late** and those **overdue now**. A payment made on time is not listed |

- Due payments and history are worked out from the recurring items and ledgers; nothing is stored for them.
- A skipped payment leaves no line.

## Splitting a bill

Tick **split this bill** on a transaction (or press **split a bill**):

1. Choose how: **equally**, **by percent** (total 100), **by shares**, or **by amounts** (total the bill).
2. Add people in the field under the lines: pick a friend, or type a name and choose **add "name" as a friend**. You are the first line.
3. Each line shows its amount as you type. **give the rest to me** fills your line. It cannot be saved until nothing is **left to assign**.

- The transaction keeps what you paid. **Budgets, statistics, reports and exports count only your part.** **details** shows who has which part.
- Each friend's part is added to what they owe you.
- The odd cent goes to the largest remainder, you first.
- Changing the amount scales every part. Deleting asks first. A shared bill cannot be an income.
- A saved bill opens as exact amounts.
- A recurring item can be split too; each payment keeps the same proportions. Payments already made keep their parts.

## Showing a bill to a friend: the bill code

- **show code** is on the bill in the list (with several friends, open **details**) and on a friend's bill line.
- A split recurring item has **show code** for the friend's part of its next payment (needs a fixed amount).
- Show the code of a payment once: before it is paid (the item's) or after (the bill's). They are different codes; a friend who takes in both owes it twice.
- The dialog shows a QR code; **copy** copies the text for your own chat.
- The name friends see is set at first run; **settings** changes it.
- The code holds the bill's name and date, your name, the friend's part, the whole bill and the currency. Nobody else's name or part.
- It is not stored and not encrypted: show it only to its friend. If too long for a QR code, use the text.

## Taking a bill from a friend

- **scan a bill** (phone) or **paste a code** (computer and phone). Nothing is written until you accept.
- The dialog shows sender, bill, date and your part. Choose the **friend** (the sender if you have one, or **new friend**).
- **Add my part to my expenses** (ticked) with a category.
- In another currency, type the rate; nothing is fetched. The converted amount can be changed.
- **Accept** makes the friend, expense and entry together, or nothing. **Deny** stores nothing. A code is not taken twice.
- If the friend corrects a bill: delete the old entry, take in the new code.
- The sender is not told.

## Reminders

- One setting for all debts: **settings**, **notifications**, **remind me about each debt**: never, every week, every 2 weeks, every month (default every week). Set per device.
- Each friend with a balance is reminded that long after the day their debt last changed, and again after as long, until settled. A new line restarts the count.
- The text holds a count, never a name or amount. The **debts** page names who is due.
- Computer: rings while meowNY is open and unlocked, at the time of day you set (09:00 default). The notifications switch turns it off.
- Phone: the next days are handed to the system, so it rings with the app closed. First use asks for the notification permission. Battery saving on some phones can delay or drop it.

## Files to send

- On a friend's page, **save statement** writes the lines and balances as **TXT**, **CSV** or **PDF**, after a **preview**. Nothing is written until **save**; then the app asks where.
- Files are plain, not encrypted, and sent to nobody. The phone has no save button.
- `meowny export debts` writes who owes what and the whole ledger.

## Backups, the phone and scripts

- Friends, entries and parts are in backups and exports. A limited export names only the friends on the bills it holds, with no balance or reminder.
- They sync with the phone. Friends of the same name added on two devices become one.
- Export parts: JSON and Excel have **debts**, **due payments** and **history of debts**; a PDF has **list of debts**, **list of due payments** and **history of debts**.
- Scripts: `meowny people|ledger|debts|splits ...`, `meowny transactions split`, `meowny recurring split`, `meowny export debts`, `debts due`, `debts history`, `debts reminders --every-days N`. Export commands take `--sections` and, for a PDF, `--include-due-payments` and `--include-debt-history`. A bill code and the ringing of a reminder need you at the app.

## What to know

- A balance is a running sum, not matched to bills.
- A code in another currency is converted once, with the rate you type, when you accept.
- A code is a notice, not a link: later edits do not follow.
