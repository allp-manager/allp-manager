#!/usr/bin/env python3
"""Release policy tests: an outage must never hide an application failure."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('validation', Path(__file__).with_name('validate-release-runs.py'))
validation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validation)
SHA = 'a' * 40


class ReleaseValidationTests(unittest.TestCase):
    def setUp(self):
        self.runs = [dict(id=index, head_sha=SHA, path='.github/workflows/' + name,
                          status='completed', conclusion='success')
                     for index, name in enumerate(validation.REQUIRED, 1)]
        self.runs.append(dict(id=4, head_sha=SHA, path='.github/workflows/backend-canary.yml',
                              status='completed', conclusion='failure'))
        self.jobs = [dict(id=index, name=name, status='completed',
                          conclusion='failure' if name == 'Snap on Ubuntu' else 'success',
                          steps=[dict(name='Check native Snap Store availability', conclusion='failure')]
                          if name == 'Snap on Ubuntu' else [])
                     for index, name in enumerate(sorted(validation.CANARY_JOBS), 1)]
        self.log = 'error: unable to contact snap store\nStore metadata via IPv4: HTTP 000\n'

    def assess(self):
        return validation.assess(self.runs, SHA, lambda _: self.jobs, lambda _: self.log)

    def test_successful_canary_needs_no_network_exception(self):
        self.runs[-1]['conclusion'] = 'success'
        self.assertEqual(self.assess()['status'], 'passed')

    def test_proven_native_outage_is_reported_as_outage(self):
        proof = self.assess()
        self.assertEqual(proof['status'], 'passed_with_store_outage')
        self.assertIn('before Allp execution', proof['store_outage'])

    def test_each_required_check_must_pass_for_exact_commit(self):
        for index in range(3):
            original = copy.deepcopy(self.runs)
            self.runs[index]['head_sha'] = 'b' * 40
            with self.assertRaises(ValueError):
                self.assess()
            self.runs = original

    def test_other_backend_failure_blocks_release(self):
        self.jobs[0]['conclusion'] = 'failure'
        with self.assertRaises(ValueError):
            self.assess()

    def test_unknown_failure_blocks_release(self):
        self.log = 'Allp parser failed\n'
        with self.assertRaises(ValueError):
            self.assess()

    def test_incomplete_jobs_block_release(self):
        self.jobs[0]['status'] = 'in_progress'
        with self.assertRaises(ValueError):
            self.assess()

    def test_cancelled_run_cannot_use_an_outage_exception(self):
        self.runs[-1]['conclusion'] = 'cancelled'
        with self.assertRaises(ValueError):
            self.assess()

    def test_mutation_started_cannot_use_network_exception(self):
        self.log += '● [1/1] Snap install started\n'
        with self.assertRaises(ValueError):
            self.assess()

    def test_only_expected_read_only_metadata_failure_is_allowed(self):
        snap = next(job for job in self.jobs if job['name'] == 'Snap on Ubuntu')
        snap['steps'][0]['name'] = 'Exercise real Snap install, refresh and removal'
        self.log = ('Install status: BackendError\nNative error: snapd returned 500: '
                    'cannot get details: got unexpected HTTP status code 408 via GET to '
                    '"https://api.snapcraft.io/v2/snaps/info/hello-world"\n')
        self.assertEqual(self.assess()['status'], 'passed_with_store_outage')
        self.log = self.log.replace('HTTP status code 408', 'HTTP status code 404')
        with self.assertRaises(ValueError):
            self.assess()

    def test_discovery_exception_requires_actual_native_failure_and_empty_results(self):
        import json
        snap = next(job for job in self.jobs if job['name'] == 'Snap on Ubuntu')
        snap['steps'][0]['name'] = 'Require complete search output'
        report = dict(schema_version=2, command='search', complete=False, results=dict(
            candidates=[], backends=[dict(backend_id='snap', state='search_failed',
                message='error: unable to contact snap store')]))
        self.log = json.dumps(report, indent=2) + '\nStore metadata via IPv4: HTTP 000\n'
        self.assertEqual(self.assess()['status'], 'passed_with_store_outage')
        report['results']['backends'][0]['message'] = 'Could not parse Snap output'
        self.log = json.dumps(report, indent=2) + '\nStore metadata via IPv4: HTTP 000\n'
        with self.assertRaises(ValueError):
            self.assess()


if __name__ == '__main__':
    unittest.main()
