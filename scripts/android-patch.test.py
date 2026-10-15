"""Tests for android-patch.py on files like the ones `tauri android init` writes. Run: python3 scripts/android-patch.test.py"""
import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("android_patch", Path(__file__).with_name("android-patch.py"))
patch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(patch)

MANIFEST = """<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:tools="http://schemas.android.com/tools">
    <uses-permission android:name="android.permission.INTERNET" />
    <application
        android:icon="@mipmap/ic_launcher"
        android:label="@string/app_name"
        android:theme="@style/Theme.meowny_desktop"
        android:usesCleartextTraffic="${usesCleartextTraffic}">
        <activity
            android:configChanges="orientation|keyboardHidden|keyboard|screenSize|locale|smallestScreenSize|screenLayout|uiMode"
            android:launchMode="singleTask"
            android:label="@string/main_activity_title"
            android:name=".MainActivity"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"""

PLAIN_ACTIVITY = """package org.meowny.app

class MainActivity : TauriActivity()
"""

EDGE_ACTIVITY = """package org.meowny.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
"""


class ManifestTests(unittest.TestCase):
    def run_on(self, text):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "AndroidManifest.xml"
            path.write_text(text, encoding="utf-8")
            first = patch.patch_manifest(path)
            once = path.read_text(encoding="utf-8")
            second = patch.patch_manifest(path)
            return first, second, once, path.read_text(encoding="utf-8")

    def test_backup_is_turned_off_once_and_the_rest_is_kept(self):
        first, second, once, twice = self.run_on(MANIFEST)
        self.assertTrue(first)
        self.assertFalse(second)
        self.assertEqual(once, twice)
        self.assertIn('android:allowBackup="false"', once)
        self.assertIn("${usesCleartextTraffic}", once)
        self.assertIn('android:name="android.permission.INTERNET"', once)
        self.assertIn('android:name=".MainActivity"', once)
        self.assertIn("xmlns:tools=", once)
        self.assertEqual(once.replace('\n        android:allowBackup="false"', ""), MANIFEST, "nothing else in the file changed")

    def test_a_backup_that_was_on_is_turned_off(self):
        first, _, once, _ = self.run_on(MANIFEST.replace("<application", '<application android:allowBackup="true"'))
        self.assertTrue(first)
        self.assertIn('android:allowBackup="false"', once)
        self.assertNotIn('android:allowBackup="true"', once)

    def test_comments_are_kept(self):
        _, _, once, _ = self.run_on(MANIFEST.replace("<application", "<!-- a note -->\n    <application"))
        self.assertIn("<!-- a note -->", once)

    def test_a_manifest_without_an_application_stops_the_build(self):
        with self.assertRaises(SystemExit):
            self.run_on('<manifest xmlns:android="http://schemas.android.com/apk/res/android"></manifest>')


class ActivityTests(unittest.TestCase):
    def run_on(self, text):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "MainActivity.kt"
            path.write_text(text, encoding="utf-8")
            first = patch.patch_activity(path)
            once = path.read_text(encoding="utf-8")
            second = patch.patch_activity(path)
            return first, second, once, path.read_text(encoding="utf-8")

    def test_a_plain_activity_gets_an_on_create_with_the_flag(self):
        first, second, once, twice = self.run_on(PLAIN_ACTIVITY)
        self.assertTrue(first)
        self.assertFalse(second)
        self.assertEqual(once, twice)
        self.assertIn("FLAG_SECURE", once)
        self.assertIn("import android.os.Bundle", once)
        self.assertIn("import android.view.WindowManager", once)
        self.assertLess(once.index("super.onCreate"), once.index("FLAG_SECURE"), "the window exists after super.onCreate")
        self.assertEqual(once.count("import android.os.Bundle"), 1)

    def test_an_activity_with_its_own_on_create_keeps_it(self):
        first, _, once, _ = self.run_on(EDGE_ACTIVITY)
        self.assertTrue(first)
        self.assertIn("enableEdgeToEdge()", once)
        self.assertEqual(once.count("super.onCreate"), 1)
        self.assertEqual(once.count("import android.os.Bundle"), 1, "an import that is there is not added twice")
        self.assertLess(once.index("super.onCreate"), once.index("FLAG_SECURE"))

    def test_an_activity_that_is_not_what_we_expect_stops_the_build(self):
        with self.assertRaises(SystemExit):
            self.run_on("package org.meowny.app\n\nclass Something\n")


if __name__ == "__main__":
    unittest.main()
