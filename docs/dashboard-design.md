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
| net worth | in accounts, debts, friends, net worth | — | 4–6 × 1–2, **6×2** |
| net worth over time | net worth by month, as a line | period | 6–12 × 2, **6×2** |
| account | one account: balance, this month, a sparkline | account | 3–6 × 1–2, **3×1** |
| accounts | every account with its balance | count | 4–6 × 2–3, **4×2** |
| budgets | each category's spent against budget, with bars | period, category, count | 6–12 × 2–4, **6×2** |
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

## 5. Default layouts

- **Desktop:** month summary 12×1; trend 12×2; net worth 6×2; budgets 6×2; stashes 6×2; upcoming 6×2; debts 6×2 — today's dashboard.
- **Phone:** month summary 2×1; trend 2×2; net worth 2×2; budgets 2×2; upcoming 2×2; stashes 2×2.

## 6. Storage

- `localStorage` key `dashboard.layout.v1.<desktop|phone>`: `{ version: 1, widgets: [{ id, type, w, h, settings }] }`.
- Read with a schema (zod): an unknown widget type or a bad value is dropped; a size outside the widget's range is clamped; an empty or broken layout falls back to the default.
- Ids of accounts, categories and stashes only, never names or amounts. Delete all data removes the keys.

## 7. Not in this work

Sharing layouts between devices, widgets made of custom queries, and more than one dashboard.
