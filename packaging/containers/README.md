# Container release checks

`container.yml` is shared by pull-request CI and the container job in
`release.yml`. Each architecture builds natively, using `ubuntu-24.04` for
amd64 and `ubuntu-24.04-arm` for arm64. No QEMU is needed. Both runners check
all package locks, then build and run the standard and LibreOffice images.

`check_image.py` checks the container's Debian architecture, every installed
package against the selected lock, and engine detection with a reported version. Whisper has no version output,
so the check executes its help command instead.
It starts the server without networking, with a read-only root, writable tmpfs
storage, dropped capabilities and no-new-privileges, then checks its health.
It needs Python 3.11+ on the host and supports Docker and Podman:

```sh
python3 -I packaging/containers/check_image.py dogubako:local --arch arm64 --runtime podman
python3 -I packaging/containers/check_image.py dogubako:office --arch arm64 --office --runtime podman
python3 -I -m unittest discover -s packaging/containers -p 'test_*.py'
```

Only a future `v*` tag's Release workflow enables publishing. PRs and main
branch CI neither log into GHCR nor push images. Opening or merging this PR
does not rerun old release tags. Do not rerun the existing alpha release to
backfill arm64: that would also rerun jobs that replace downloadable assets.
Obtain the owner's approval for the exact new tag and source commit before
triggering a release. Registry visibility is a separate owner decision.

After both image configurations pass on each runner, the workflow pushes the
**same tested standard image**, without rebuilding it, under a unique staging
tag `build-<run-id>-<attempt>-<arch>`. These staging tags are registry writes too.
The merge job consumes immutable digest references from this run's artifacts.
It checks the proposed index before assigning `<version>` and checks the index
read back from GHCR afterwards. The index must contain exactly linux/amd64 and
linux/arm64, each pointing to its tested digest. Docker and Podman select the
matching architecture when pulling the version tag. No `latest` tag is changed.
The staging tags are retained; any cleanup should preserve digests referenced
by version indexes.

Only the standard image is published. LibreOffice and rembg remain local build
options. This change does not modify the existing alpha image, release tags,
downloads, compose settings, Podman's Quadlet unit or SELinux labels. PR #19's
shared `:z` bind-mount labels and compose/Quadlet health checks still apply to
prebuilt images. The mount-free smoke check does not retest SELinux host policy.

To inspect a newly approved release after publishing:

```sh
docker buildx imagetools inspect ghcr.io/mischa-dev/dogubako:<new-version>
podman manifest inspect ghcr.io/mischa-dev/dogubako:<new-version>
```

On each native host, pull the version tag without `--platform`, then run
`check_image.py` with that host's architecture. Authenticate first while the
package is private. A correct manifest does not make the package publicly
accessible.
