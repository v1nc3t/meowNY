# A dashboard you arrange

Status: being built on branch `feat/tidy-ui`.

## In short

- The dashboard is a grid of widgets. The app ships with a default layout: today's dashboard.
- You add, remove, move and resize widgets, and give each one its period, account or category.
- The computer and the phone each keep their own layout, on that device only.
- One look for every widget: a title line, one main value or visual, nothing else.

## 1. The grid

- **Desktop:** 12 columns. A widget spans 3, 4, 6, 8, 9 or 12 columns, and 1 to 4 rows. A row is `--dash-row` tall (about 9rem).
- **Phone:** 2 columns. A widget spans 1 or 2 columns, and 1 to 4 rows.
- Widgets are placed in order and pack without holes (`grid-auto-flow: row dense`).
- Below 48em on the desktop the grid drops to 6 columns and every span is halved, rounded up (a 3 becomes 2… a 12 becomes 6).
- Sizes in rem only; the gap is `--page-gap`.

## 2. Edit mode

1. Open **⋯** › **customize dashboard**.
2. Each widget shows a handle (drag to move), a size button, a settings button and a remove button.
3. **Size** opens a small grid of cells: hover or arrow keys pick how many columns and rows it takes; Enter or a click sets it.
   A handle at the widget's bottom-right corner resizes it by dragging; it snaps to whole cells.
4. **Add widget** opens the catalog: each widget with its name, one line and its allowed sizes. Choosing one adds it at the end.
5. **Reset to default** puts the default layout back (after a confirm).
6. **Done** leaves edit mode. Every change is saved at once; Escape also leaves.

Keyboard: a focused widget moves with `Alt` + arrow keys and resizes with `Alt` + `Shift` + arrow keys.
Screen readers hear the widget's name, position and size.

## 3. Settings of a widget

Only the settings that make sense for it, in a small dialog:

- **period:** this month, last month, this year, last year, last 12 months, or a custom range (from, to). "This month" follows the month you step to in the dashboard's period control, unless set otherwise.
- **account:** all accounts or one (or a few, where it fits).
- **category:** all, or one or a few, by side (expense / income).
- **count:** how many rows, for lists (5, 10, 20).
- **title:** your own name for it, optional.

A setting that points to something deleted or archived falls back to "all" and says so in the dialog.

## 4. The widgets

Each has a minimum and maximum size; the default size in bold.

| Widget | What it shows | Settings | Sizes (desktop) |
|---|---|---|---|
| month summary | income, expense, savings, savings rate, each with its change | period, account | 6–12 × 1, **12×1** |
| trend | income, expense and savings by month, as lines | period, account, category | 6–12 × 2–3, **12×2** |
| cash flow | money in and out by month, as bars | period, account | 6–12 × 2, **6×2** |
| net worth | in accounts, debts, friends, net worth | — | 4–6 × 1–2, **6×1** |
| net worth over time | net worth by month, as a line | period | 6–12 × 2, **6×2** |
| account | one account: balance, this month, a sparkline | account | 3–6 × 1–2, **3×1** |
| accounts | every account with its balance | count | 4–6 × 2–3, **4×2** |
| budgets | each category's spent against budget, with bars | period, category, count | 6–12 × 2–4, **6×3** |
| category | one category: spent, budget, left, a bar and a sparkline | period, category | 3–6 × 1–2, **3×1** |
| spending by category | the biggest categories, as bars with amounts | period, account, count | 4–6 × 2–3, **4×2** |
| top expenses | the biggest transactions | period, account, category, count | 4–6 × 2–3, **4×2** |
| recent transactions | the newest transactions | account, category, count | 4–8 × 2–4, **6×2** |
| upcoming | recurring items due in the next 30 days, and overdue ones | count | 4–6 × 2–3, **4×2** |
| stashes | every stash with its bar | count | 4–6 × 2–3, **6×2** |
| stash | one stash: saved, target, bar, date | (stash) | 3–4 × 1, **3×1** |
| debts | friends who owe you and whom you owe | count | 4–6 × 2, **4×2** |
| loans | what you owe on loans, the next installment, payoff | — | 4–6 × 1–2, **4×1** |
| savings rate | the rate of the period as a large number and a bar | period, account | 3–4 × 1, **3×1** |

The phone uses the same widgets; a desktop span of 6 or more becomes 2 columns, less becomes 1.

Every widget is its title line and then one main value or one visual, with at most a short list in columns (a name, then its value at the right). A widget
measures its own width and height (container queries) and drops what is secondary when it is small: the period at the right of the title in a narrow one,
a word that only names a figure in a one-row one, the category and day columns of a list when it is narrow, the loans list under the figures unless it is
tall. The default height of a widget is the one its content fits in, so a short widget (the net worth, a figure with a bar) is one row. Where a widget has
"all" for a choice it does not make sense for (a one-account widget without an account), it shows the main account, the biggest category or the first stash.

## 5. Default layouts

- **Desktop**, row by row on the 12 columns:
  - month summary 12×1;
  - trend 8×2, with savings rate 4×1 and net worth 4×1 beside it;
  - spending by category 4×2, upcoming 4×2, accounts 4×2;
  - budgets 6×4, with cash flow 6×2, loans 6×1 and the main account 6×1 beside it;
  - stashes 6×2, debts 6×2;
  - recent transactions 8×2, top expenses 4×2.
- **Phone:** month summary 2×1; savings rate 1×1 and loans 1×1; trend 2×2; net worth 2×1; spending by category 2×2; budgets 2×4;
  upcoming 2×2; stashes 2×2; recent transactions 2×2.

## 6. Storage

- `localStorage` key `dashboard.layout.v1.<desktop|phone>`: `{ version: 1, widgets: [{ id, type, w, h, settings }] }`.
- Read with a schema (zod): an unknown widget type or a bad value is dropped; a size outside the widget's range is clamped; an empty or broken layout falls back to the default.
- Ids of accounts, categories and stashes only, never names or amounts. Delete all data removes the keys.

## 7. Not in this work

Sharing layouts between devices, widgets made of custom queries, and more than one dashboard.
