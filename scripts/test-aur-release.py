#!/usr/bin/env python3
"""AUR publication must follow a matching Stable release and never downgrade."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('aur_release', Path(__file__).with_name('resolve-aur-release.py'))
aur_release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aur_release)
SHA = 'a' * 40


class AurReleaseTests(unittest.TestCase):
    def setUp(self):
        self.release = dict(tag_name='v0.6.7', draft=False, prerelease=False,
            html_url='https://github.com/allp-manager/allp-manager/releases/tag/v0.6.7',
            assets=[dict(name='allp-release-manifest.json')])
        self.event = dict(workflow_run=dict(name='Release', conclusion='success', event='push',
            head_sha=SHA, head_repository=dict(full_name=aur_release.REPO)))
        self.aur = dict(results=[dict(Name='allp-manager', Version='0.6.5-1',
            URL='https://github.com/allp-manager/allp-manager')])

    def select(self, name='workflow_run', ref='refs/heads/main', tag='', publish=False):
        return aur_release.select(name, self.event, self.release, SHA, ref, tag, publish, self.aur)

    def test_successful_release_automatically_builds_and_publishes(self):
        result = self.select()
        self.assertTrue(result['build'])
        self.assertTrue(result['publish'])

    def test_scheduled_retry_uses_published_stable(self):
        self.assertTrue(self.select(name='schedule')['publish'])

    def test_branch_and_failed_release_runs_do_not_publish(self):
        for field, value in (('event', 'pull_request'), ('head_sha', 'b' * 40),
                             ('conclusion', 'failure')):
            original = copy.deepcopy(self.event)
            self.event['workflow_run'][field] = value
            self.assertFalse(self.select()['build'])
            self.event = original
        self.event['workflow_run']['head_repository']['full_name'] = 'someone/fork'
        self.assertFalse(self.select()['build'])

    def test_prereleases_drafts_and_unpublished_artifacts_are_rejected(self):
        for field in ('draft', 'prerelease'):
            self.release[field] = True
            with self.assertRaises(ValueError):
                self.select()
            self.release[field] = False
        self.release['assets'] = []
        with self.assertRaises(ValueError):
            self.select()

    def test_equal_and_newer_aur_versions_are_idempotent(self):
        for version in ('0.6.7-1', '0.6.7-2', '0.6.10-1', '1.0.0-1'):
            self.aur['results'][0]['Version'] = version
            result = self.select(name='schedule')
            self.assertFalse(result['build'])
            self.assertFalse(result['publish'])

    def test_manual_preview_can_build_on_a_test_branch_but_cannot_publish(self):
        result = self.select(name='workflow_dispatch', ref='refs/heads/prerelease-test', tag='v0.6.7')
        self.assertTrue(result['build'])
        self.assertFalse(result['publish'])
        with self.assertRaises(ValueError):
            self.select(name='workflow_dispatch', ref='refs/heads/prerelease-test', tag='v0.6.7', publish=True)

    def test_tag_origin_and_package_identity_must_match(self):
        with self.assertRaises(ValueError):
            self.select(name='workflow_dispatch', tag='v0.6.6')
        self.aur['results'][0]['URL'] = 'https://github.com/someone/other'
        with self.assertRaises(ValueError):
            self.select()


if __name__ == '__main__':
    unittest.main()
