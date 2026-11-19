"""Regression cases for refusing incomplete or incorrectly labelled releases."""
import copy
import importlib.util
import json
import subprocess
import tomllib
from pathlib import Path
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, HERE / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


index_check = load('check_index')
image_check = load('check_image')


class IndexTests(unittest.TestCase):
    def setUp(self):
        self.digests = ['sha256:' + c * 64 for c in 'ab']
        self.refs = ['ghcr.io/mischa-dev/dogubako@' + d for d in self.digests]
        self.index = {
            'schemaVersion': 2,
            'mediaType': 'application/vnd.oci.image.index.v1+json',
            'manifests': [
                {'digest': digest, 'platform': {'os': 'linux', 'architecture': arch}}
                for arch, digest in zip(['amd64', 'arm64'], self.digests)
            ],
        }

    def test_oci_and_docker_indexes(self):
        index_check.check(self.index, self.refs)
        self.index['mediaType'] = 'application/vnd.docker.distribution.manifest.list.v2+json'
        self.index['manifests'].reverse()
        self.index['manifests'][0]['platform']['variant'] = 'v8'
        index_check.check(self.index, self.refs)

    def test_missing_or_extra_platform(self):
        for entries in [self.index['manifests'][:1], self.index['manifests'] * 2]:
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                index_check.check(dict(self.index, manifests=entries), self.refs)

    def test_mislabelled_duplicate_wrong_os_or_variant(self):
        for field, value in [('architecture', 'amd64'), ('architecture', 'arm'),
                             ('os', 'windows'), ('variant', 'v9')]:
            index = copy.deepcopy(self.index)
            index['manifests'][1]['platform'][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                index_check.check(index, self.refs)

    def test_digest_swap(self):
        self.index['manifests'][0]['digest'] = self.digests[1]
        with self.assertRaises(ValueError):
            index_check.check(self.index, self.refs)

    def test_mutable_source_tag(self):
        with self.assertRaises(ValueError):
            index_check.check(self.index, ['ghcr.io/mischa-dev/dogubako:latest', self.refs[1]])

    def test_single_arch_manifest(self):
        self.index['mediaType'] = 'application/vnd.oci.image.manifest.v1+json'
        with self.assertRaises(ValueError):
            index_check.check(self.index, self.refs)


class ImageTests(unittest.TestCase):
    def test_standard_and_office_images_on_both_architectures(self):
        manifest = tomllib.loads((image_check.ROOT / 'packaging/engines/engines.toml').read_text())
        for arch in ['amd64', 'arm64']:
            for office in [False, True]:
                pins = {}
                for suffix in [arch] + ([f'office-{arch}'] if office else []):
                    path = image_check.ROOT / f'packaging/engines/docker-packages-{suffix}.txt'
                    pins.update(line.split('=', 1) for line in path.read_text().splitlines()
                                if line and not line.startswith('#'))
                # All known engines are found. Whisper intentionally has no version.
                deps = [{'id': dep, 'found': True, 'version': None if dep == 'whisper' else '1'}
                        for engine in manifest['engine'].values() for dep in engine['deps']]
                outputs = [arch, '\n'.join(f'{p}={v}' for p, v in pins.items()),
                           json.dumps(deps), '', '[]', 'container-id']
                with self.subTest(arch=arch, office=office):
                    with patch.object(image_check, 'run', side_effect=outputs), patch.object(
                            image_check.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
                        image_check.check('docker', 'example', arch, office)

    def test_wrong_architecture_stops_before_package_checks(self):
        with patch.object(image_check, 'run', return_value='amd64') as run:
            with self.assertRaisesRegex(ValueError, 'expected arm64'):
                image_check.check('podman', 'example', 'arm64', False)
            self.assertEqual(run.call_count, 1)

    def test_missing_or_wrong_packages_block_release(self):
        for packages in ['other=1', 'ffmpeg=wrong-version']:
            with self.subTest(packages=packages):
                with patch.object(image_check, 'run', side_effect=['arm64', packages]):
                    with self.assertRaisesRegex(ValueError, 'ffmpeg: expected'):
                        image_check.check('docker', 'example', 'arm64', False)

    def test_pinned_packages_are_insufficient_without_engines(self):
        path = image_check.ROOT / 'packaging/engines/docker-packages-arm64.txt'
        pins = '\n'.join(s for s in path.read_text().splitlines() if s and not s.startswith('#'))
        with patch.object(image_check, 'run', side_effect=['arm64', pins, '[]']):
            with self.assertRaisesRegex(ValueError, 'unrunnable engines'):
                image_check.check('docker', 'example', 'arm64', False)


if __name__ == '__main__':
    unittest.main()
