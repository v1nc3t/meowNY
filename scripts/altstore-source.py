#!/usr/bin/env python3
"""Writes the AltStore (and SideStore) source for a release: the file that tells AltStore on an iPhone where the newest IPA is.
The format is https://faq.altstore.io/developers/make-a-source.

Each release carries it as altstore.json, so this one address always gives the newest version:
    https://github.com/v1nc3t/meowNY/releases/latest/download/altstore.json
Added once in AltStore (Browse, Sources, +), it then offers every new release as an update.

AltStore refuses an app whose bundle id, versions or permissions differ from what the source says, so all of those are read
from the Info.plist inside the IPA itself. The IPA is unsigned (AltStore signs it on the phone), so it has no entitlements.

Usage: python3 scripts/altstore-source.py VERSION IPA_FILE > altstore.json
"""
import datetime
import json
import os
import plistlib
import re
import sys
import zipfile

REPO = 'https://github.com/v1nc3t/meowNY'


def info_plist(ipa):
    with zipfile.ZipFile(ipa) as z:
        names = [n for n in z.namelist() if re.fullmatch(r'Payload/[^/]+\.app/Info\.plist', n)]
        if len(names) != 1:
            raise SystemExit(f'{ipa}: expected one Payload/*.app/Info.plist, found {len(names)}')
        return plistlib.loads(z.read(names[0]))


def source(version, ipa):
    info = info_plist(ipa)
    if info['CFBundleShortVersionString'] != version:
        raise SystemExit(f"{ipa} is version {info['CFBundleShortVersionString']}, not {version}")
    privacy = {k: v for k, v in info.items() if k.startswith('NS') and k.endswith('UsageDescription')}
    entry = {
        'version': info['CFBundleShortVersionString'],
        'buildVersion': info['CFBundleVersion'],
        'date': datetime.date.today().isoformat(),
        'downloadURL': f'{REPO}/releases/download/v{version}/{os.path.basename(ipa)}',
        'size': os.path.getsize(ipa),
    }
    if 'MinimumOSVersion' in info:
        entry['minOSVersion'] = info['MinimumOSVersion']
    return {
        'name': 'meowNY',
        'subtitle': 'A budget app that keeps your data on your own devices.',
        'identifier': f"{info['CFBundleIdentifier']}.source",
        'sourceURL': f'{REPO}/releases/latest/download/altstore.json',
        'website': REPO,
        'tintColor': '#166534',
        'nsfw': False,
        'apps': [
            {
                'name': 'meowNY',
                'bundleIdentifier': info['CFBundleIdentifier'],
                'developerName': 'meowNY',
                'subtitle': 'Budgets on your phone, synced with your computer.',
                'localizedDescription': 'Budgets, transactions, recurring bills, debts and documents, encrypted on your phone '
                'and synced with your computer over your own wifi. No server, no account, no telemetry.',
                'iconURL': 'https://raw.githubusercontent.com/v1nc3t/meowNY/main/src-tauri/icons/logo-ios.png',
                'tintColor': '#166534',
                'category': 'utilities',
                'versions': [entry],
                'appPermissions': {'entitlements': [], 'privacy': privacy},
            }
        ],
        'news': [],
    }


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    json.dump(source(sys.argv[1], sys.argv[2]), sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write('\n')
