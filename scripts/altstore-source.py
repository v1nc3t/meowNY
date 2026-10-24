#!/usr/bin/env python3
"""Writes the AltStore (and SideStore) source for a release: the file that tells AltStore on an iPhone where the newest IPA is.

Each release carries it as altstore.json, so this one address always gives the newest version:
    https://github.com/v1nc3t/meowNY/releases/latest/download/altstore.json
Added once in AltStore (Browse, Sources, +), it then offers every new release as an update.

The permissions listed must match the app's own (src-tauri/Info.ios.plist), or AltStore may refuse to install it.

Usage: python3 scripts/altstore-source.py VERSION IPA_FILE > altstore.json
"""
import datetime
import json
import os
import plistlib
import sys

REPO = 'https://github.com/v1nc3t/meowNY'
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def source(version, ipa):
    with open(os.path.join(ROOT, 'src-tauri', 'tauri.conf.json'), encoding='utf-8') as f:
        bundle_id = json.load(f)['identifier']
    with open(os.path.join(ROOT, 'src-tauri', 'Info.ios.plist'), 'rb') as f:
        privacy = {k: v for k, v in plistlib.load(f).items() if k.startswith('NS') and k.endswith('UsageDescription')}
    return {
        'name': 'meowNY',
        'identifier': f'{bundle_id}.source',
        'sourceURL': f'{REPO}/releases/latest/download/altstore.json',
        'website': REPO,
        'apps': [
            {
                'name': 'meowNY',
                'bundleIdentifier': bundle_id,
                'developerName': 'meowNY',
                'subtitle': 'A budget app that keeps your data on your own devices.',
                'localizedDescription': 'Budgets, transactions, recurring bills, debts and documents, encrypted on your phone '
                'and synced with your computer over your own wifi. No server, no account, no telemetry.',
                'iconURL': 'https://raw.githubusercontent.com/v1nc3t/meowNY/main/src-tauri/icons/logo-ios.png',
                'tintColor': '#166534',
                'versions': [
                    {
                        'version': version,
                        'buildVersion': version,
                        'date': datetime.date.today().isoformat(),
                        'downloadURL': f'{REPO}/releases/download/v{version}/{os.path.basename(ipa)}',
                        'size': os.path.getsize(ipa),
                    }
                ],
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
