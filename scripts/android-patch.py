#!/usr/bin/env python3
"""Hardens the Android project that `tauri android init` generates (src-tauri/gen/android, which is not kept in git).

What it changes, and why (docs/sync-design.md section 13):
  * AndroidManifest.xml: `android:allowBackup="false"`. Android's backup would copy the app's folder to the person's Google
    account, with the encrypted data, `vault.json` and the secret file side by side; the erase after 5 wrong passphrases would
    then protect nothing. A phone that is lost or replaced gets its data back by pairing with the desktop.
  * MainActivity.kt: `FLAG_SECURE` on the window. The screen cannot be captured, and the picture of the app in the list of recent
    apps is blank, so no amounts or names show there.

Both changes are made once: running the script again changes nothing. It stops with an error when a file is not what it expects,
so a build never goes on with the protection missing.

Usage: python3 scripts/android-patch.py [path to src-tauri/gen/android]
"""
import re
import sys
from pathlib import Path


def patch_manifest(path: Path) -> bool:
    """Sets allowBackup to false on <application>, leaving the rest of the file exactly as it is. Returns whether it changed."""
    text = path.read_text(encoding="utf-8")
    found = re.search(r"<application\b[^>]*>", text, re.S)
    if found is None:
        raise SystemExit(f"{path}: there is no <application> element")
    tag = found.group(0)
    if re.search(r'android:allowBackup\s*=\s*"false"', tag):
        return False
    if re.search(r"android:allowBackup\s*=", tag):
        patched = re.sub(r'(android:allowBackup\s*=\s*)"[^"]*"', r'\1"false"', tag)
    else:
        patched = tag.replace("<application", '<application\n        android:allowBackup="false"', 1)
    path.write_text(text[: found.start()] + patched + text[found.end() :], encoding="utf-8")
    return True


SECURE = "window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)"


def patch_activity(path: Path) -> bool:
    """Makes the window secure. Returns whether the file changed."""
    text = path.read_text(encoding="utf-8")
    if "FLAG_SECURE" in text:
        return False
    imports = ["import android.os.Bundle", "import android.view.WindowManager"]
    if re.search(r"override\s+fun\s+onCreate\s*\(", text):
        # The generated activity already has onCreate: add the line right after the call to super.
        text, count = re.subn(r"(\n([ \t]*)super\.onCreate\([^)]*\)[ \t]*)", lambda m: f"{m.group(1)}\n{m.group(2)}{SECURE}", text, count=1)
        if count != 1:
            raise SystemExit(f"{path}: onCreate has no call to super.onCreate")
    else:
        text, count = re.subn(
            r"class\s+MainActivity\s*:\s*TauriActivity\(\)[ \t]*(\{\s*\})?",
            "class MainActivity : TauriActivity() {\n  override fun onCreate(savedInstanceState: Bundle?) {\n    super.onCreate(savedInstanceState)\n    " + SECURE + "\n  }\n}",
            text,
            count=1,
        )
        if count != 1:
            raise SystemExit(f"{path}: class MainActivity : TauriActivity() was not found")
    # Imports go after the package line, the way Kotlin wants them, and only when they are missing.
    missing = [line for line in imports if line not in text]
    if missing:
        text, count = re.subn(r"(^package [^\n]+\n)", lambda m: m.group(1) + "\n" + "\n".join(missing) + "\n", text, count=1, flags=re.M)
        if count != 1:
            raise SystemExit(f"{path}: there is no package line")
    path.write_text(text, encoding="utf-8")
    return True


def find_one(root: Path, pattern: str) -> Path:
    found = sorted(root.glob(pattern))
    if len(found) != 1:
        raise SystemExit(f"{root}: expected one file matching {pattern}, found {len(found)}")
    return found[0]


def main(argv: list[str]) -> None:
    root = Path(argv[1] if len(argv) > 1 else "src-tauri/gen/android")
    if not root.is_dir():
        raise SystemExit(f"{root} is not a folder: run `npx tauri android init` first")
    manifest = find_one(root, "app/src/main/AndroidManifest.xml")
    activity = find_one(root, "app/src/main/java/**/MainActivity.kt")
    print(f"{manifest}: {'changed' if patch_manifest(manifest) else 'already done'}")
    print(f"{activity}: {'changed' if patch_activity(activity) else 'already done'}")


if __name__ == "__main__":
    main(sys.argv)
