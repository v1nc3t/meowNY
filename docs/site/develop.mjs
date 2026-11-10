// The develop page: building meowNY, its tests, how the code is laid out, and how a release is made. Short; the code
// and its comments hold the rest.

import { REPO, SPONSOR } from './render.mjs'

const file = (path) => `[\`${path}\`](${REPO}/blob/main/${path})`

const SECTIONS = [
  {
    id: 'requirements',
    title: 'Requirements',
    blocks: [
      {
        t: 'ul',
        items: [
          'Node.js 22 or newer; Rust stable (1.90 or newer) from [rustup.rs](https://rustup.rs).',
          "[Tauri's system libraries](https://v2.tauri.app/start/prerequisites/). Debian and Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential pkg-config`.",
          'A keychain (GNOME Keyring or KWallet, Windows Credential Manager, macOS Keychain). Without one the first run fails with "key unavailable".',
        ],
      },
    ],
  },
  {
    id: 'build',
    title: 'Build and run',
    blocks: [
      { t: 'code', text: 'npm install\nnpm run tauri dev      # the app; edits to ui/src reload, edits to src-tauri/src rebuild\nnpm run tauri build    # a release build and installers for this system' },
      {
        t: 'table',
        head: ['Program', 'Source'],
        rows: [
          ['`meowny-desktop`, the app (window and installers say meowNY)', '`src-tauri/`'],
          ['`meowny`, the command line', '`cli/`: `cargo install --path cli`'],
        ],
      },
      { t: 'h3', id: 'android', text: 'Android' },
      { t: 'p', text: 'Java 21, the Android SDK and NDK 27 (`sdkmanager "ndk;27.2.12479018"`, with `ANDROID_HOME`, `NDK_HOME` and the NDK compilers on the `PATH`), and `rustup target add aarch64-linux-android`.' },
      {
        t: 'ol',
        items: [
          'Make the Android project (`src-tauri/gen/android`, not kept in git): `npx tauri android init`.',
          'Patch it, again after every init (no Android backup, no screenshots, an R8 rule for the camera): `python3 scripts/android-patch.py`.',
          'Remove the old APK, because a debug build grows it in place: `rm -f src-tauri/gen/android/app/build/outputs/apk/universal/debug/*.apk`.',
          'Build: `npx tauri android build --apk --target aarch64 --debug`.',
          'Install on a phone with USB debugging on: `adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`.',
        ],
      },
      { t: 'h3', id: 'ios', text: 'iPhone' },
      { t: 'p', text: `A Mac with Xcode and \`rustup target add aarch64-apple-ios\`. ${file('src-tauri/Info.ios.plist')} holds the camera, photo and local network texts.` },
      {
        t: 'ol',
        items: ['Make the Xcode project: `npx tauri ios init`.', 'Build the unsigned IPA: `npx tauri ios build --target aarch64 --no-sign`. It lands in `src-tauri/gen/apple/build/arm64/meowNY.ipa`.'],
      },
    ],
  },
  {
    id: 'tests',
    title: 'Tests and checks',
    blocks: [
      { t: 'p', text: '`npm run check` runs what CI runs on every pull request (the UI lint, build and tests, the website, the network call list, the Android script, the Rust tests of the app and the command line), and clippy on both Rust programs. It stops at the first failure. It builds the Rust code, so it takes a while. The parts, one by one:' },
      {
        t: 'code',
        text: 'cd src-tauri && cargo test          # the Rust core\ncd cli && cargo test                # the command line, against the real app code\nnpm run lint --workspace ui && npm test --workspace ui\nnpm run check:docs                  # the website\nnpm run check:outbound              # fails on new network code',
      },
      {
        t: 'ul',
        items: [
          'After a route changes: `cd cli && cargo run --example dump_schema > schema.json`.',
          'After a command changes: `cd cli && UPDATE_DOCS=1 cargo test --offline --test docs`, then `npm run docs`. The command line page is made from real output, and a test fails until it is recorded again.',
          'CI (`.github/workflows/ci.yml`) runs all of this on every pull request, builds the desktop, Android and iPhone apps, audits the dependencies, and on `main` publishes this website. Releases are `release.yml`.',
        ],
      },
    ],
  },
  {
    id: 'layout',
    title: 'How the code is laid out',
    blocks: [
      {
        t: 'code',
        text: 'ui/          React UI (Vite, TypeScript, Zod); every screen calls Rust through src/api/invoke.ts\nsrc-tauri/   the app and Rust core: database, vault, sync, documents, debts, the command API\n  migrations/  numbered SQL files, never edited once shipped\ncli/         the meowny command line (Rust, one dependency)\ndocs/        the design notes, and site/, which builds this website\nscripts/     checks and helpers: outbound check, android-patch, logo, release',
      },
      {
        t: 'ul',
        items: [
          'Rust owns the database, the keys and every rule. The command line is a client of the running app over its local API, and its route table is one table in Rust.',
          'Money is integer minor units (`*_minor`). A schema change is the next numbered migration, appended to `MIGRATIONS` in `src-tauri/src/migrate.rs`.',
          'Files: Rust `snake_case` (`x.rs` plus `x/`, tests in `x_tests.rs`), React `PascalCase.tsx`, other TypeScript `camelCase.ts` with `name.test.ts` beside it.',
          `Design notes: ${file('docs/sync-design.md')}, ${file('docs/documents-design.md')}, ${file('docs/debts-design.md')}.`,
        ],
      },
    ],
  },
  {
    id: 'add-feature',
    title: 'Add a feature, step by step',
    blocks: [
      { t: 'p', text: 'A feature touches the same places, in the same order. Skip a step that does not apply.' },
      {
        t: 'ol',
        items: [
          '**Schema.** A new column or table is the next numbered file in `src-tauri/migrations/`, appended to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Never edit one that has shipped: restoring an old backup replays the migrations in order (`run_up_to`), so a changed one breaks old backups.',
          '**Rust logic.** Put the rule in its module in `src-tauri/src/` (`recurring.rs`, `debts/` and so on), with tests beside it: `x_tests.rs`, or a `tests` module at the bottom of a small file. Money is integer minor units (`*_minor`).',
          '**App command.** Add a thin `#[tauri::command]` function to `src-tauri/src/commands.rs` that calls the logic, and list it in `generate_handler!` in `src-tauri/src/lib.rs`.',
          '**Command line.** If the command line should have it too, add a route over the same function in `src-tauri/src/api/` (`read.rs`, `write.rs`, `confirmed.rs`, `debts.rs` or `documents.rs`) and list it in `ALL` in `src-tauri/src/api/routes.rs`. The command itself comes from the route.',
          '**Record the command.** `cd cli && cargo run --example dump_schema > schema.json`. Add an example of the new command to the tour in `cli/tests/docs.rs`. Then `cd cli && UPDATE_DOCS=1 cargo test --offline --test docs` and `npm run docs`.',
          '**Phone sync.** Only if the data goes to the phone: remake the table\'s sync-stamp and `updated_at` triggers with the new column in them (see `0014_category_order.sql`; a new table follows `0012_debts.sql`), add the field to its row in `src-tauri/src/sync/wire.rs`, raise `PROTOCOL` there, and add it to section 9 of `docs/sync-design.md`. Two devices with different schema or protocol versions refuse to sync and are told to update.',
          '**Screen.** Add the call to the matching module in `ui/src/api/` (it goes through `ui/src/api/invoke.ts`, which checks the answer against a Zod schema). Build the page from the shared list pieces in `ui/src/components/RowParts.tsx` (`Table`, `Row`, `DayGroups`, `TapRow`) and `useOpenRow`; one page serves the computer and the phone. A new page also goes in `ui/src/pages/index.ts`, `ui/src/App.tsx` and `ui/src/components/AppSidebar.tsx` (and `PhoneTabs.tsx` if the phone has it).',
          '**Texts.** Every text is a key in `ui/src/i18n/en.ts` and in the six other files there (`de`, `es`, `fr`, `it`, `pt`, `ro`). A test fails if one is missing.',
          '**Guide.** Write how to use it as steps in `docs/site/guide.mjs`, in the section where it belongs. If the app now stores or sends something new, add it to the privacy tables there (browser storage included). Update the design note the change touches in `docs/`, and keep its section numbers: code cites them.',
          '**Check.** Run `npm run check`.',
        ],
      },
    ],
  },
  {
    id: 'demo',
    title: 'Demo data',
    blocks: [
      { t: 'p', text: 'Sixteen months of made-up data, dated from today: budgets, recurring items, debts, and awkward text for the exports. Quit the app, then:' },
      { t: 'code', text: 'cd src-tauri\ncargo run --example dev_vault -- ~/.local/share/org.meowny.app   # deletes the app data there first\ncargo run --example api_demo                                     # the command API on demo data, no app needed' },
      { t: 'p', text: 'The website\'s screenshots are this demo data in a debug build (`MEOWNY_PROFILE=phone` with `MEOWNY_DEV_PASSPHRASE` for the phone\'s screens), at 1280 × 800 and 400 × 860, each in light and dark: `docs/site/images/<name>-light.png` and `<name>-dark.png`, named in `docs/site/shots.mjs`. Run it with its own `HOME` and keychain, so your own data is never touched.' },
    ],
  },
  {
    id: 'release',
    title: 'Releasing',
    blocks: [
      {
        t: 'ol',
        items: [
          'Check that CI is green on `main`.',
          'On GitHub open **Actions › Release › Run workflow**, pick **patch** (0.2.0 to 0.2.1), **minor** or **major**, and run it. From a terminal: `gh workflow run release.yml -f bump=patch`.',
          'Wait, about 30 minutes. CI sets the version everywhere, records the command line docs, commits and tags on `main`, then builds and publishes.',
          'Run `git pull` to get the version commit.',
        ],
      },
      { t: 'p', text: 'By hand, on Linux and on an up-to-date `main`: `npm run release patch`, then `git push origin main v<version>`. Releases come from `main` only: the script refuses another branch, and the workflow refuses a tag whose commit is not on `main`.' },
      { t: 'p', text: `${file('.github/workflows/release.yml')} builds the Linux \`.deb\` and AppImage, the Windows installer, a signed APK, the unsigned IPA, the command line for Linux and Windows, \`SHA256SUMS\`, a build attestation per file and \`altstore.json\`. If a build fails, fix it and release the next patch version.` },
      { t: 'h3', id: 'android-key', text: 'Once: the Android release key' },
      { t: 'p', text: 'Every Android version must be signed with the same key, or phones refuse the update.' },
      {
        t: 'ol',
        items: ['Make the key with `keytool`, outside the repository. Keep it, with its password, in a password manager.', 'Give it to GitHub as three secrets with `gh secret set`: the key (as base64), its password and its alias.'],
      },
      { t: 'code', label: 'both steps', text: 'keytool -genkeypair -v -keystore ~/meowny-release.keystore -alias meowny \\\n  -keyalg RSA -keysize 4096 -validity 10000 -storetype PKCS12\nbase64 -w0 ~/meowny-release.keystore | gh secret set ANDROID_KEYSTORE_BASE64\ngh secret set ANDROID_KEYSTORE_PASSWORD\ngh secret set ANDROID_KEY_ALIAS --body meowny' },
      { t: 'p', text: 'Not done yet: signed Windows installers (a paid certificate), macOS (notarisation needs a paid Apple account), updates from inside the desktop app.' },
    ],
  },
  {
    id: 'contributing',
    title: 'Contributing',
    blocks: [
      {
        t: 'ol',
        items: [
          'Open an issue before a large change.',
          'Run the checks above. A bug fix comes with a test that fails without it.',
          'Keep dependencies few, and say why when you add one.',
          'Keep it local-first: a new network call is on purpose, off until the user turns it on, and listed in `scripts/outbound-allowlist.txt`.',
          'A new desktop feature ships its command line commands in the same change ([add a feature](#add-feature)).',
          'Update the design note the change touches, and the privacy part of the guide (`docs/site/guide.mjs`) when what is stored or sent changes.',
        ],
      },
      { t: 'p', text: `The same rules: [CONTRIBUTING.md](${REPO}/blob/main/CONTRIBUTING.md). A security problem: do not open a public issue, see [SECURITY.md](${REPO}/blob/main/SECURITY.md).` },
    ],
  },
]

export function buildDevelop() {
  return {
    file: 'develop.html',
    title: 'Develop meowNY',
    description: 'Build meowNY, run its tests, find your way in the code, and make a release.',
    lead: 'Build it, test it, change it, release it.',
    footer: `meowNY is free software (AGPL-3.0). [Source](${REPO}) · [Support on Ko-fi](${SPONSOR}): it goes toward the App Store and Google Play.`,
    nav: [{ title: 'Develop', links: SECTIONS.map((s) => ({ id: s.id, text: s.title })) }],
    sections: SECTIONS,
  }
}
