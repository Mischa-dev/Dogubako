#!/usr/bin/env python3
"""Verify a native Docker/Podman image without host mounts or network access."""
import argparse
import json
from pathlib import Path
import subprocess
import time
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[2]


def run(*args):
    return subprocess.check_output(args, text=True, timeout=120).strip()


def check(runtime, image, arch, office):
    def container(*args, entrypoint=None):
        flags = ['--entrypoint', entrypoint] if entrypoint else []
        return run(runtime, 'run', '--rm', '--network=none', *flags, image, *args)

    actual = container('--print-architecture', entrypoint='dpkg')
    if actual != arch:
        raise ValueError(f'expected {arch}, got {actual}')
    packages = container('-W', '-f=${Package}=${Version}\n', entrypoint='dpkg-query')
    installed = dict(line.split('=', 1) for line in packages.splitlines())
    pins = {}
    for suffix in [arch] + ([f'office-{arch}'] if office else []):
        path = ROOT / f'packaging/engines/docker-packages-{suffix}.txt'
        for line in path.read_text().splitlines():
            if line and not line.startswith('#'):
                package, version = line.split('=', 1)
                if package in pins and pins[package] != version:
                    raise ValueError(f'conflicting pin for {package}')
                pins[package] = version
    mismatches = [f'{p}: expected {v}, got {installed.get(p)}'
                  for p, v in pins.items() if installed.get(p) != v]
    if mismatches:
        raise ValueError('\n'.join(mismatches))

    manifest = tomllib.loads((ROOT / 'packaging/engines/engines.toml').read_text())
    expected = set()
    for engine in manifest['engine'].values():
        option = engine.get('docker_option')
        if engine['tier'] == 'dropped' or (option and not (office and option == 'office')):
            continue
        if engine.get('debian') or engine.get('docker_build') or 'piper' in engine['deps']:
            expected.update(engine['deps'])
    deps = {d['id']: d for d in json.loads(container('--json', 'deps'))}
    missing = [dep for dep in sorted(expected)
               if not deps.get(dep, {}).get('found')
               or (dep != 'whisper' and not deps[dep].get('version'))]
    if missing:
        raise ValueError(f'missing or unrunnable engines: {missing}')
    # whisper.cpp's --help has no version string. Execute it explicitly instead.
    container('-ec', "whisper-cli --help > /tmp/whisper-help 2>&1; grep -q 'usage:' /tmp/whisper-help",
              entrypoint='sh')
    # Exercise the CLI and server as the image's unprivileged user.
    container('tools', 'list', '--json')
    name = 'dogubako-check-' + uuid.uuid4().hex
    try:
        run(runtime, 'run', '-d', '--name', name, '--network=none', '--read-only',
            '--cap-drop=ALL', '--security-opt=no-new-privileges',
            '--tmpfs', '/tmp:rw,nosuid,nodev',
            '--tmpfs', '/data:rw,nosuid,nodev,uid=10001,gid=10001,mode=0700', image)
        for _ in range(30):
            result = subprocess.run([runtime, 'exec', name, 'dogubako', 'server', 'health'],
                                    capture_output=True, text=True, timeout=10)
            if result.returncode == 0:
                break
            time.sleep(2)
        else:
            raise ValueError('server did not become healthy')
    finally:
        subprocess.run([runtime, 'logs', name], check=False)
        subprocess.run([runtime, 'rm', '-f', name], check=False)
    print(f'{image}: {arch}, {len(pins)} pinned packages, {len(expected)} engines, server healthy')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image')
    parser.add_argument('--arch', required=True, choices=['amd64', 'arm64'])
    parser.add_argument('--office', action='store_true')
    parser.add_argument('--runtime', default='docker', choices=['docker', 'podman'])
    args = parser.parse_args()
    check(args.runtime, args.image, args.arch, args.office)
