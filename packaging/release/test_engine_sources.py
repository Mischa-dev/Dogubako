"""Offline regression tests. Run: python3 -I -m unittest discover -s packaging/release -p 'test_*.py'."""

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


sources = load('sources', 'packaging/release/engine_sources.py')
locking = load('locking', 'packaging/engines/lock.py')


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.data = b'old upstream source'
        self.src = {'url': 'https://example.org/RELEASE_1_2/src/example-1.2.tar.gz',
                    'sha256': hashlib.sha256(self.data).hexdigest()}

    def test_nss_and_homebrew_revision_survive_current_version_changes(self):
        lock = json.loads((ROOT / 'packaging/engines/engines.lock.json').read_text())
        src = lock['bottles']['nss']['source']
        self.assertIn('/NSS_3_132_RTM/src/nss-3.132.tar.gz', src['url'])
        # No metadata lookup or version substitution is allowed at release time.
        with patch.object(sources, 'get', return_value=self.data) as get:
            pinned = {**src, 'sha256': hashlib.sha256(self.data).hexdigest()}
            name, digest, _ = sources.fetch_source('nss', '3.132_7', pinned, self.root)
        get.assert_called_once_with(src['url'], timeout=300)
        self.assertTrue(name.startswith('nss-3.132_7-'))
        self.assertEqual(digest, pinned['sha256'])

    def test_formula_source_keeps_non_version_path_and_query(self):
        url = 'https://example.org/RELEASE_1_2/src/file.tar.gz?revision=abc'
        formula = {'name': 'example', 'versions': {'stable': 'unrelated'},
                   'urls': {'stable': {'url': url, 'checksum': self.src['sha256']}}}
        self.assertEqual(locking.formula_source(formula), {'url': url, 'sha256': self.src['sha256']})

    def test_upstream_hash_mismatch_writes_nothing(self):
        with patch.object(sources, 'get', return_value=b'new version'):
            with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                sources.fetch_source('example', '1.2_4', self.src, self.root)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_same_version_different_source_archive_is_rejected(self):
        recipe = b'url "https://example.org/tag.tar.gz"\nsha256 "' + b'b'*64 + b'"\n'
        review = {'recipe_sha256': hashlib.sha256(recipe).hexdigest()}
        with self.assertRaisesRegex(RuntimeError, 'source is absent'):
            sources.verify_recipe('example', {'source': self.src}, review, recipe)
        with self.assertRaisesRegex(RuntimeError, 'reviewed inputs'):
            sources.verify_recipe('example', {'source': self.src}, review, recipe+b'changed')

    def test_query_does_not_become_filename(self):
        with patch.object(sources, 'get', return_value=self.data):
            name, _, _ = sources.fetch_source('example', '1.2', {**self.src, 'url': self.src['url']+'?rev=abc'}, self.root)
        self.assertNotIn('?', name)

    def test_floating_or_short_git_refs_rejected_before_network(self):
        for rev in ['main', 'v1.2', 'abcdef0', 'a'*39, 'g'*40]:
            with self.subTest(rev=rev), patch.object(sources.subprocess, 'run') as run:
                with self.assertRaises(ValueError):
                    sources.fetch_source('git', 'r123', {'git': 'https://example.org/repo.git', 'revision': rev}, self.root)
                run.assert_not_called()

    def test_formula_metadata_rejects_floating_revision(self):
        with self.assertRaises(SystemExit):
            locking.formula_source({'name': 'example', 'urls': {'stable': {
                'url': 'https://example.org/repo.git', 'revision': 'main'}}})

    def test_git_formula_records_commit_instead_of_mutable_tag(self):
        revision = 'a'*40
        self.assertEqual(locking.formula_source({'name': 'example', 'urls': {'stable': {
            'url': 'https://example.org/repo.git', 'using': 'git', 'tag': 'stable', 'revision': revision}}}),
            {'git': 'https://example.org/repo.git', 'revision': revision, 'tag': 'stable'})

    def test_stale_lock_rejects_short_hashes(self):
        manifest = {'engine': {}}
        for src in [{'url': self.src['url'], 'sha256': 'abc'},
                    {'git': 'https://example.org/repo.git', 'revision': 'main'}]:
            self.assertTrue(locking.stale(manifest, {'engines': {}, 'bottles': {'example': {'source': src}}}))

    def test_git_archive_uses_exact_commit_even_if_branch_moves(self):
        repo = self.root / 'repo'
        repo.mkdir()
        def git(*args):
            return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.DEVNULL).decode().strip()
        git('init', '-q')
        git('config', 'user.email', 'test@example.org')
        git('config', 'user.name', 'Test')
        (repo / 'source').write_text('old')
        git('add', '.')
        git('commit', '-qm', 'old')
        rev = git('rev-parse', 'HEAD')
        (repo / 'source').write_text('new')
        git('commit', '-qam', 'new')
        # Redirect only the transport for an entirely local test. Production pins remain HTTPS.
        run = subprocess.run
        def local_run(args, **kwargs):
            return run([str(repo) if a == 'https://example.org/repo.git' else a for a in args], **kwargs)
        with patch.object(sources.subprocess, 'run', side_effect=local_run):
            name, _, note = sources.fetch_source('git', 'r123', {'git': 'https://example.org/repo.git', 'revision': rev}, self.root)
        with tarfile.open(self.root / name) as t:
            self.assertEqual(t.extractfile('git-'+rev[:12]+'/source').read(), b'old')
        self.assertIn(rev, note)

    def fixture(self):
        lock = {'engines': {'example': {'tier': 'bundled', 'formulae': ['example']}},
                'bottles': {'example': {'version': '1.2_4', 'license': 'GPL-3.0-only',
                                      'sha256': 'b'*64, 'source': self.src}}}
        inputs = {'example': {'bottle_sha256': 'b'*64, 'recipe_sha256': 'c'*64,
                              'source': self.src, 'inputs': []}}
        lp, ip = self.root/'lock.json', self.root/'inputs.json'
        lp.write_text(json.dumps(lock))
        ip.write_text(json.dumps(inputs))
        return lock, inputs, lp, ip

    def invoke(self, lp, ip, *args):
        with patch.object(sys, 'argv', ['sources', '--version', 'test', '--lock', str(lp),
                                      '--inputs', str(ip), '--out', str(self.root/'out'), *args]):
            return sources.main()

    def test_changed_bottle_or_source_requires_new_review(self):
        lock, _, lp, ip = self.fixture()
        for key, value in [('sha256', 'd'*64), ('source', {**self.src, 'url': 'https://example.org/new'})]:
            changed = json.loads(json.dumps(lock))
            changed['bottles']['example'][key] = value
            lp.write_text(json.dumps(changed))
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'review source inputs'):
                self.invoke(lp, ip, '--check')

    def test_check_is_offline(self):
        _, _, lp, ip = self.fixture()
        with patch.object(sources, 'get', side_effect=AssertionError('network')):
            self.assertEqual(self.invoke(lp, ip, '--check'), 0)

    def test_failure_does_not_overwrite_existing_archive(self):
        _, inputs, lp, ip = self.fixture()
        inputs['example']['inputs'] = [{'url': 'https://example.org/patch', 'sha256': 'd'*64}]
        ip.write_text(json.dumps(inputs))
        out = self.root/'out'
        out.mkdir()
        old = out/'engine-sources-test.tar'
        old.write_bytes(b'previous complete archive')
        with patch.object(sources, 'get', return_value=self.data), patch.object(sources, 'fetch_recipe'):
            self.assertEqual(self.invoke(lp, ip), 1)
        self.assertEqual(old.read_bytes(), b'previous complete archive')
        self.assertEqual(list(out.iterdir()), [old])

    def test_success_excludes_stale_files_and_includes_inputs_and_lock(self):
        _, _, lp, ip = self.fixture()
        stale = self.root/'out/engine-sources'
        stale.mkdir(parents=True)
        (stale/'old.tar.gz').write_bytes(b'stale')
        with patch.object(sources, 'get', return_value=self.data), patch.object(sources, 'fetch_recipe'):
            self.assertEqual(self.invoke(lp, ip), 0)
        with tarfile.open(self.root/'out/engine-sources-test.tar') as t:
            self.assertFalse(any('old.tar.gz' in n for n in t.getnames()))
            self.assertEqual(t.extractfile('engine-sources-test/engines.lock.json').read(), lp.read_bytes())
            self.assertIn('engine-sources-test/source-inputs.json', t.getnames())


if __name__ == '__main__':
    unittest.main()
