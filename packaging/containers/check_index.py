#!/usr/bin/env python3
"""Check that an OCI index/Docker manifest list selects exactly the tested images."""
import json
from pathlib import Path
import re
import sys


def check(index, references):
    if index.get('schemaVersion') != 2 or index.get('mediaType') not in {
        'application/vnd.oci.image.index.v1+json',
        'application/vnd.docker.distribution.manifest.list.v2+json',
    }:
        raise ValueError('expected an OCI index or Docker manifest list')
    if len(references) != 2 or any(not re.fullmatch(
            r'ghcr\.io/mischa-dev/dogubako@sha256:[0-9a-f]{64}', ref) for ref in references):
        raise ValueError('expected two immutable Dogubako digest references')
    expected = dict(zip(['amd64', 'arm64'], [ref.split('@')[1] for ref in references]))
    manifests = index.get('manifests', [])
    if len(manifests) != 2:
        raise ValueError('expected exactly two platform manifests')
    seen = set()
    for manifest in manifests:
        platform = manifest.get('platform', {})
        arch = platform.get('architecture')
        if platform.get('os') != 'linux' or arch not in expected or arch in seen:
            raise ValueError(f'unexpected or duplicate platform: {platform}')
        if platform.get('variant', '') not in (['', 'v8'] if arch == 'arm64' else ['']):
            raise ValueError(f'unsupported platform variant: {platform}')
        if manifest.get('digest') != expected[arch]:
            raise ValueError(f'{arch} does not reference its tested image')
        seen.add(arch)
    print('index selects the tested linux/amd64 and linux/arm64 images')


if __name__ == '__main__':
    check(json.loads(Path(sys.argv[1]).read_text()), sys.argv[2:])
