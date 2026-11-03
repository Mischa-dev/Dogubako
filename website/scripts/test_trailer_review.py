import hashlib
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from trailer_review import ASSETS, validate


class TrailerReviewTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'trailer/dist').mkdir(parents=True)
        (self.root / 'website/download').mkdir(parents=True)
        (self.root / 'website/download/index.html').write_text(
            '<a href="https://github.com/Mischa-dev/Dogubako/releases/download/v0.1.0-alpha.1/app.dmg">Alpha</a>')
        self.info = {'status': 'reviewed', 'release': 'v0.1.0-alpha.1', 'sha256': {}}
        for relative in ASSETS:
            data = relative.encode()
            (self.root / 'trailer' / relative).write_bytes(data)
            self.info['sha256'][relative] = hashlib.sha256(data).hexdigest()
        self.write_manifest()

    def write_manifest(self):
        (self.root / 'trailer/dist/review-status.json').write_text(json.dumps(self.info))

    def test_reviewed_release_and_assets_pass(self):
        validate(self.root)

    def test_missing_or_invalid_manifest_fails(self):
        path = self.root / 'trailer/dist/review-status.json'
        for content in ('not json', '[]', 'null'):
            with self.subTest(content=content):
                path.write_text(content)
                with self.assertRaises(ValueError):
                    validate(self.root)
        path.unlink()
        with self.assertRaises(ValueError):
            validate(self.root)

    def test_in_development_is_not_approval(self):
        self.info['status'] = 'in-development'
        self.write_manifest()
        with self.assertRaises(ValueError):
            validate(self.root)

    def test_different_release_fails(self):
        self.info['release'] = 'v0.1.0-alpha.2'
        self.write_manifest()
        with self.assertRaises(ValueError):
            validate(self.root)

    def test_missing_hashes_or_unexpected_paths_fail(self):
        for hashes in ({}, {ASSETS[0]: '0' * 64}, {'../outside': '0' * 64}):
            with self.subTest(hashes=hashes):
                self.info['sha256'] = hashes
                self.write_manifest()
                with self.assertRaises(ValueError):
                    validate(self.root)

    def test_changed_or_missing_assets_fail(self):
        for relative in ASSETS:
            with self.subTest(relative=relative):
                path = self.root / 'trailer' / relative
                original = path.read_bytes()
                path.write_bytes(b'changed')
                with self.assertRaises(ValueError):
                    validate(self.root)
                path.unlink()
                with self.assertRaises(ValueError):
                    validate(self.root)
                path.write_bytes(original)

    def test_copy_removes_stale_assets_when_validation_fails(self):
        scripts = self.root / 'website/scripts'
        scripts.mkdir()
        for name in ('trailer.sh', 'trailer_review.py'):
            shutil.copy(Path(__file__).parent / name, scripts / name)
        result = subprocess.run(['sh', str(scripts / 'trailer.sh')], capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        for relative in ASSETS:
            self.assertTrue((self.root / 'website/trailer' / Path(relative).name).exists())
        self.info['sha256'] = {}
        self.write_manifest()
        result = subprocess.run(['sh', str(scripts / 'trailer.sh')], capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list((self.root / 'website/trailer').iterdir()), [])


if __name__ == '__main__':
    unittest.main()
