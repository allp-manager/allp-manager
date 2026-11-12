#!/usr/bin/env python3
"""Select a published Stable release for a tested, monotonic AUR update."""
import argparse
import json
from pathlib import Path
import re

REPO = 'allp-manager/allp-manager'
STABLE = r'v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'


def select(event_name, event, release, commit, ref, requested_tag, publish, aur):
    tag = release['tag_name']
    if not re.fullmatch(STABLE, tag) or release['draft'] or release['prerelease']:
        raise ValueError('Only a published Stable vX.Y.Z release can enter AUR')
    if release['html_url'] != f'https://github.com/{REPO}/releases/tag/{tag}':
        raise ValueError('Unexpected release origin')
    if not re.fullmatch(r'[a-f0-9]{40}|[a-f0-9]{64}', commit):
        raise ValueError('Invalid release commit')
    if 'allp-release-manifest.json' not in {asset['name'] for asset in release['assets']}:
        raise ValueError('Published release manifest is missing')
    result = dict(tag=tag, commit=commit, build=True, publish=False, reason='Published Stable release')
    if event_name == 'workflow_dispatch':
        if requested_tag != tag:
            raise ValueError('Requested tag does not match the published release')
        if publish and ref != 'refs/heads/main':
            raise ValueError('Manual publication must run from main')
        result['publish'] = publish
    elif event_name in ('workflow_run', 'schedule'):
        if ref != 'refs/heads/main':
            raise ValueError('Automatic publication must run trusted main code')
        if event_name == 'workflow_run':
            run = event['workflow_run']
            if (run['name'] != 'Release' or run['conclusion'] != 'success'
                    or run['event'] != 'push' or run['head_repository']['full_name'] != REPO
                    or run['head_sha'] != commit):
                result.update(build=False, reason='Completed run does not match the latest published Stable release')
                return result
        result['publish'] = True
    else:
        raise ValueError('Unsupported publication event')
    entries = aur['results']
    if len(entries) != 1 or entries[0]['Name'] != 'allp-manager' or entries[0]['URL'] != f'https://github.com/{REPO}':
        raise ValueError('AUR package identity does not match this project')
    version = tag[1:]
    current = entries[0]['Version']
    match = re.fullmatch(r'([0-9]+\.[0-9]+\.[0-9]+)-[0-9]+(?:\.[0-9]+)?', current)
    if match is None:
        raise ValueError('Unexpected current AUR version; review before publishing')
    current_parts = tuple(map(int, match[1].split('.')))
    target_parts = tuple(map(int, version.split('.')))
    if result['publish'] and current_parts >= target_parts:
        result.update(build=False, publish=False, reason=f'AUR {current} already contains this version or a newer one')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--event-name', required=True)
    parser.add_argument('--event', type=Path, required=True)
    parser.add_argument('--release', type=Path, required=True)
    parser.add_argument('--aur', type=Path, required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--ref', required=True)
    parser.add_argument('--tag', default='')
    parser.add_argument('--publish', choices=('true', 'false'), default='false')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = select(args.event_name, json.loads(args.event.read_text()),
                        json.loads(args.release.read_text()), args.commit, args.ref,
                        args.tag, args.publish == 'true', json.loads(args.aur.read_text()))
    except (ValueError, KeyError) as error:
        parser.exit(1, str(error) + '\n')
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
