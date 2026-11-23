# A tidy UI: columns, no loose text, a menu behind the logo on the phone

Status: being built on branch `feat/tidy-ui`.

## 1. The problem

Rows on the phone (and some on the desktop) carry loose text: a name, an amount, then "set aside €3,900.00", "≈ €10.01 / month",
"checking · 16/11/2026 · +€1,706.46" stacked under each other. Every fact is shown at once, nothing lines up, and the eye has no
place to rest. Pages also open with rows of buttons, checkboxes and explaining sentences before the list. The phone spends a bar at
the top on six icons and a bar at the bottom on five tabs.

## 2. The goal

The look of calm finance and task apps (a bank app's account list, a budgeting app's category list, an issue tracker's tables): each list
is a set of columns that line up, a row shows the two or three facts you scan for, and everything else is one tap away. The app's own
character stays: the pixel logo and titles, the mono font, the green accent, light and dark themes, the tokens in `tokens.css`.

## 3. Rules (every page, desktop and phone)

1. **One fact per cell.** A table cell holds one value. No second line of grey text under a name or an amount (no "available €680.10",
   no "≈ €10.01 / month", no "not in net worth", no "reminder came 16/11/2026"). Such a fact gets a column of its own when it is worth
   scanning for, a small mark (an icon or a short badge of one or two words, the `Badge` component) when it is a state, or goes to
   the row's details (rule 4).
2. **Columns line up.** Amounts are right aligned with tabular figures in a column of the same width on every row; names are at the
   left and cut with an ellipsis, never wrapped. Rows have one height in a list.
3. **No loose sentences.** No paragraph that explains a page or a field. An explanation that is worth keeping goes behind a help
   button (`HelpButton`: a small "?" icon button that opens a popover with the text). Fields keep their label and show a line under
   them only for an error. Empty states keep their one short line and their button.
4. **Details one tap away.** What is not in the row's columns is in its details: on the desktop the panel under the row (the chevron
   of `Row`, as now) or the page of the thing (an account's page, a stash's page); on the phone the panel the row opens (`TapRow`).
   Details are a list of label → value pairs (`Facts`), then the row's buttons. Never a run of words joined by dots.
5. **A quiet top of the page.** A page starts with `PageHeader`: its title, at most two buttons (the main action filled, one more
   plain), and a "more" button (`MoreMenu`, the "⋯" icon) that holds the rest: second actions (scan, paste a code, add friend,
   notifications), and switches such as "show removed friends" or "show archived". Filters and search stay where the list needs them,
   in one line: a search field and a filter button that opens the filters.
6. **Fewer words on buttons.** A button whose icon says it all is an icon button with an `aria-label` (edit, delete, archive, restore,
   mark as paid). Buttons with words have one or two words.
7. **Dialogs too.** Dialogs follow rules 3 and 6: no explaining paragraphs (a `HelpButton` beside the title when needed), one short line
   of warning at most where something cannot be taken back.
8. **Same on every page.** The same components and the same spacing everywhere; nothing page-local that looks like them.

## 4. The phone shell

- **No bar at the bottom and no bar of icons at the top.** `PhoneTabs` and the icon row of the phone header go away.
- **The logo is the menu button.** Each page's `PageHeader` starts, on a phone, with the logo as a button (`aria-label` "menu",
  `aria-expanded`, `aria-controls` the drawer) at the left of the title. The header row is sticky at the top of the page, on the page's
  background, with a hairline under it once the page has scrolled.
- **The drawer.** The logo opens a panel that slides in from the left over the page (`min(80vw, 300px)` wide, full height, a dim
  backdrop): the logo and the word "meowny" at its top, then every page with its icon in the desktop sidebar's order (dashboard,
  transactions, accounts, stashes, categories, recurring, debts, documents, sync, settings — export is not on a phone), the current one
  marked as in the desktop sidebar, and lock at the bottom. It is a modal `<dialog>`: the focus stays inside, Escape, a tap on the
  backdrop, a swipe to the left, or choosing a page closes it, and the focus goes back to the logo. It slides in 200 ms and does not
  move with `prefers-reduced-motion`.
- **Swipes.** A swipe to the right that starts at the left edge of the screen (the first 24 px) opens the drawer. The swipe from page to
  page (it followed the bottom tabs) goes; the pages' own swipes (the budget halves) stay.
- The round add button (`QuickAdd`) stays, as it is.

## 5. The phone rows (`TapRow`)

- **The closed row** has at most three parts on one line, in columns: the name (with at most one mark, e.g. an "overdue" badge or the
  "shared" icon), the amount at the right, and the chevron. Under the name there may be **one** short grey value — the one fact you
  scan for in that list (a transaction's category, a recurring item's next due date, an account's type) — never two joined by dots,
  and never under the amount. The `sub` prop with its dot-joined parts goes; `TapRow` gets `meta` (one value) instead.
- **The open row** shows `Facts` (label at the left in grey, value at the right) and then its buttons, as icons with words in one row
  (or a row of icon buttons when there are more than three).
- Lists on the phone are flat: no box around each group, a hairline between rows, the group's name as a small heading with its total
  at the right of the heading line.

## 6. Shared pieces (in `ui/src/components/`)

| Piece | What |
|---|---|
| `PageHeader` | `title`, `actions` (≤ 2), `more` (items for `MoreMenu`), optional `aside` (a total such as "in accounts €13,308.93"); renders the logo menu button on a phone; sticky on a phone |
| `MoreMenu` | the "⋯" icon button and its menu: `role="menu"`, items are buttons or switches (`role="menuitemcheckbox"`), arrow keys move, Escape closes and gives the focus back, a click outside closes |
| `HelpButton` | "?" icon button with `aria-label` "help", opens a popover (on a phone: a small dialog) with a title and a few lines |
| `Facts` | a `<dl>` of label → value pairs in two columns, values right aligned, amounts with tabular figures |
| `PhoneMenu` | the drawer of section 4 |
| `TapRow` | as in section 5 |

## 7. Not in this work

New features, new data, changes to the core or the CLI. Every action that exists keeps a way to reach it, and the keyboard shortcuts
and focus rules stay as they are.
