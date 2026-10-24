# Releasing

One command makes a release, and pushing its tag builds and publishes every installer. The workflow is
[.github/workflows/release.yml](../.github/workflows/release.yml).

| File | For |
|---|---|
| `meowNY_<v>_amd64.deb` | Debian, Ubuntu, Mint, Kali. Installs `meowny-desktop` and the `meowny` command line |
| `meowNY_<v>_amd64.AppImage` | Any other Linux |
| `meowNY_<v>_x64-setup.exe` | Windows 10 and 11 |
| `meowNY_<v>_android.apk` | Android 7 and newer, 64-bit ARM (nearly every phone since 2017) |
| `meowNY_<v>_iphone.ipa` | iPhone, through AltStore or SideStore |
| `meowny-<v>-linux-x86_64`, `meowny-<v>-windows-x86_64.exe` | The command line on its own |
| `altstore.json` | The AltStore source, so iPhones get updates |
| `SHA256SUMS` | Checksums of all of the above |

## Once: the Android release key

Every version of the Android app must be signed with the same key, or phones refuse the update and people have to
uninstall (and pair again). Make the key once, outside the repository:

```sh
keytool -genkeypair -v -keystore ~/meowny-release.keystore -alias meowny \
  -keyalg RSA -keysize 4096 -validity 10000 -storetype PKCS12
```

Give it to GitHub as three secrets:

```sh
base64 -w0 ~/meowny-release.keystore | gh secret set ANDROID_KEYSTORE_BASE64
gh secret set ANDROID_KEYSTORE_PASSWORD     # the password you chose, typed at the prompt
gh secret set ANDROID_KEY_ALIAS --body meowny
```

Keep the keystore file and its password in a password manager as well. A lost key cannot be replaced: the next Android
version would have to be installed fresh on every phone.

## Each release

On `main`, with everything merged and CI green:

```sh
npm run release 0.2.0
git push origin HEAD v0.2.0
```

`npm run release` sets the version in `src-tauri/tauri.conf.json`, both `Cargo.toml` files and their lock files, records
the command line docs again (they print the version), and commits and tags. The push starts the release workflow, which
takes about half an hour (Windows is the slowest). When it is done, the release is on the
[releases page](https://github.com/v1nc3t/meowNY/releases) with install notes.

If a build fails, fix it on `main` and release the next patch version (`0.2.1`). A tag that never became a release can
also be moved instead: `git tag -f -a v0.2.0 -m "meowNY 0.2.0" && git push -f origin v0.2.0`.

## Updating an installed app

| | Update |
|---|---|
| Linux | `sudo apt install ./meowNY_<v>_amd64.deb`, or replace the AppImage |
| Windows | Run the new installer; it replaces the old version and keeps the data |
| Android | [Obtainium](https://github.com/ImranR98/Obtainium) with this repository's address checks for new releases and installs them. Opening a new APK by hand works too |
| iPhone | Add `https://github.com/v1nc3t/meowNY/releases/latest/download/altstore.json` as a source in AltStore or SideStore once; it then offers each new version |

The data folder is not touched by an update. Back up before updating anyway while the app is a pre-release.

## Not done yet

- **Signing of the desktop installers.** Windows shows a SmartScreen warning until the installer is signed (a
  certificate, a few hundred USD a year). Linux needs none.
- **macOS.** Left out of the release for now; a `.dmg` that opens without a warning needs an Apple Developer account
  (99 USD a year) for notarisation.
- **Updates from inside the desktop app** (phase 11: signed updates, the check as a visible setting).
