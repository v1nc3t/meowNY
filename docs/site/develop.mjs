// The develop page: building meowNY, its tests, how the code is laid out, and how a release is made. Short; the code
// and its comments hold the rest.

import { REPO } from './render.mjs'

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
      { t: 'code', text: 'npx tauri android init                 # src-tauri/gen/android, not kept in git\npython3 scripts/android-patch.py       # again after every init: no Android backup, no screenshots\nnpx tauri android build --apk --target aarch64 --debug' },
      { t: 'h3', id: 'ios', text: 'iPhone' },
      { t: 'p', text: `A Mac with Xcode and \`rustup target add aarch64-apple-ios\`. ${file('src-tauri/Info.ios.plist')} holds the camera, photo and local network texts.` },
      { t: 'code', text: 'npx tauri ios init\nnpx tauri ios build --target aarch64 --no-sign   # src-tauri/gen/apple/build/arm64/meowNY.ipa' },
    ],
  },
  {
    id: 'tests',
    title: 'Tests and checks',
    blocks: [
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
      { t: 'p', text: `With CI green on \`main\`: GitHub, **Actions › Release › Run workflow**, pick **patch** (0.2.0 to 0.2.1), **minor** or **major**, and run. Or from a terminal:` },
      { t: 'code', text: 'gh workflow run release.yml -f bump=patch' },
      { t: 'p', text: 'CI sets the version everywhere, records the command line docs, commits and tags on `main`, then builds and publishes (about 30 minutes). `git pull` afterwards to get the version commit. By hand, on Linux and on an up-to-date `main`: `npm run release patch`, then `git push origin main v<version>`. Releases come from `main` only: the script refuses another branch, and the workflow refuses a tag whose commit is not on `main`.' },
      { t: 'p', text: `${file('.github/workflows/release.yml')} builds the Linux \`.deb\` and AppImage, the Windows installer, a signed APK, the unsigned IPA, the command line for Linux and Windows, \`SHA256SUMS\`, a build attestation per file and \`altstore.json\`. If a build fails, fix it and release the next patch version.` },
      { t: 'h3', id: 'android-key', text: 'Once: the Android release key' },
      { t: 'p', text: 'Every Android version must be signed with the same key, or phones refuse the update. Make it outside the repository and keep it, with its password, in a password manager:' },
      { t: 'code', text: 'keytool -genkeypair -v -keystore ~/meowny-release.keystore -alias meowny \\\n  -keyalg RSA -keysize 4096 -validity 10000 -storetype PKCS12\nbase64 -w0 ~/meowny-release.keystore | gh secret set ANDROID_KEYSTORE_BASE64\ngh secret set ANDROID_KEYSTORE_PASSWORD\ngh secret set ANDROID_KEY_ALIAS --body meowny' },
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
          'A new desktop feature ships its command line commands in the same change.',
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
    footer: `meowNY is free software (AGPL-3.0). [Source](${REPO}).`,
    nav: [{ title: 'Develop', links: SECTIONS.map((s) => ({ id: s.id, text: s.title })) }],
    sections: SECTIONS,
  }
}
