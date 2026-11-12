#!/usr/bin/env python3
"""Require exact-source CI; distinguish a proven pre-mutation Store outage."""
import argparse
import json
from pathlib import Path
import re
import subprocess

REQUIRED = ('ci.yml', 'windows-winget.yml', 'termux-smoke.yml')
CANARY_JOBS = {'apt on Ubuntu', 'dnf on Fedora', 'pacman on Arch Linux',
               'Flatpak on Ubuntu', 'Snap on Ubuntu'}


class ChecksPending(ValueError):
    """Exact-source workflows have not completed yet; publication must wait."""


def store_outage(job, raw_log):
    failed = [step['name'] for step in job['steps'] if step.get('conclusion') == 'failure']
    log = re.sub(r'\x1b\[[0-9;]*m', '', raw_log)
    log = re.sub(r'^\ufeff?\d{4}-\d\d-\d\dT\S+Z ', '', log, flags=re.M)
    if any(marker in log for marker in ('Snap install started', 'Snap remove started',
                                        'Snap upgrade started', 'Snap finished in')):
        return None
    if failed == ['Check native Snap Store availability']:
        if 'error: unable to contact snap store' in log and (
            re.search(r'curl: \(28\).*api\.snapcraft\.io.*[Tt]imeout', log)
            or 'Store metadata via IPv4: HTTP 000' in log
        ):
            return 'Native Snap Store connection failed before Allp execution.'
    if failed == ['Require complete search output']:
        start = log.find('{\n  "schema_version":')
        if start < 0:
            return None
        try:
            report, _ = json.JSONDecoder().raw_decode(log[start:])
            backends = report['results']['backends']
            if (report['command'] == 'search' and report['complete'] is False
                    and not report['results']['candidates'] and len(backends) == 1
                    and backends[0]['backend_id'] == 'snap'
                    and backends[0]['state'] == 'search_failed'
                    and backends[0]['message'].strip() == 'error: unable to contact snap store'
                    and 'error: unable to contact snap store' in log
                    and 'Store metadata via IPv4: HTTP 000' in log):
                return 'Native Snap Store outage prevented read-only discovery.'
        except (KeyError, TypeError, ValueError):
            return None
    if failed == ['Exercise real Snap install, refresh and removal']:
        if ('Install status: BackendError' in log
                and re.search(r'Native error: snapd returned 500:.*HTTP status code 408 via GET to '
                              r'"https://api\.snapcraft\.io/v2/snaps/info/', log)):
            return 'Snap Store returned HTTP 408 during read-only exact validation; installation never started.'
    return None


def assess(runs, sha, fetch_jobs, fetch_log):
    matching = [run for run in runs if run.get('head_sha') == sha]
    proof = {'commit': sha, 'required_runs': {}, 'status': 'passed'}
    for workflow in REQUIRED:
        relevant = [run for run in matching if run.get('path', '').split('@')[0]
                    == '.github/workflows/' + workflow]
        found = [run for run in relevant if run.get('conclusion') == 'success']
        if not found:
            if not relevant or any(run.get('status') != 'completed' for run in relevant):
                raise ChecksPending(f'Waiting for {workflow} on {sha}')
            raise ValueError(f'No successful {workflow} run for {sha}')
        proof['required_runs'][workflow] = max(found, key=lambda run: run['id'])['id']
    canaries = [run for run in matching if run.get('path', '').split('@')[0]
                == '.github/workflows/backend-canary.yml']
    passed = [run for run in canaries if run.get('conclusion') == 'success']
    if passed:
        proof['canary_run'] = max(passed, key=lambda run: run['id'])['id']
        return proof
    completed = [run for run in canaries if run.get('status') == 'completed']
    if not completed:
        raise ChecksPending('Waiting for the exact-source backend canary run')
    canary = max(completed, key=lambda run: run['id'])
    if canary.get('conclusion') != 'failure':
        raise ValueError('Backend canary did not complete with a supported outcome')
    jobs = fetch_jobs(canary['id'])
    if len(jobs) != len(CANARY_JOBS) or {job['name'] for job in jobs} != CANARY_JOBS:
        raise ValueError('Backend canary job set is incomplete or unexpected')
    for job in jobs:
        expected = 'failure' if job['name'] == 'Snap on Ubuntu' else 'success'
        if job['status'] != 'completed' or job['conclusion'] != expected:
            raise ValueError(f"Backend canary requires attention: {job['name']}")
    snap = next(job for job in jobs if job['name'] == 'Snap on Ubuntu')
    reason = store_outage(snap, fetch_log(snap['id']))
    if reason is None:
        raise ValueError('Snap canary failure is not a proven pre-mutation Store outage')
    proof.update(status='passed_with_store_outage', canary_run=canary['id'],
                 canary_job=snap['id'], store_outage=reason)
    return proof


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--sha', required=True)
    parser.add_argument('--runs', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', args.repo):
        parser.error('invalid repository')
    if not re.fullmatch(r'[a-f0-9]{40}|[a-f0-9]{64}', args.sha):
        parser.error('invalid source commit')

    def api(path):
        return subprocess.check_output(['gh', 'api', f'repos/{args.repo}/actions/{path}'],
                                       text=True, timeout=90)

    try:
        proof = assess(json.loads(args.runs.read_text())['workflow_runs'], args.sha,
                       lambda run: json.loads(api(f'runs/{run}/jobs?per_page=100'))['jobs'],
                       lambda job: api(f'jobs/{job}/logs'))
    except ChecksPending as error:
        parser.exit(75, str(error) + '\n')
    except (ValueError, KeyError) as error:
        parser.exit(1, str(error) + '\n')
    args.output.write_text(json.dumps(proof, indent=2) + '\n')
    if proof['status'] == 'passed_with_store_outage':
        print('::warning::' + proof['store_outage'] + ' Canary remains failed; see release validation evidence.')
    print(json.dumps(proof, indent=2))


if __name__ == '__main__':
    main()
