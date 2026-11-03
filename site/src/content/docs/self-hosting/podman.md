---
title: Podman
description: Run Dogubako with Podman instead of Docker, with the same compose file, SELinux, rootless, starting at boot and a Quadlet unit.
sidebar:
  order: 3
---

Podman runs the same image and the same compose file as
[Docker](/self-hosting/docker/). On Fedora, RHEL and CentOS Stream it is
already installed, so there is no need to add Docker. This page covers what
is different.

## Start it

Rootful and rootless Podman keep separate image and container storage, so pick
one and run every `podman` command that way. An image built without `sudo`
can't be seen by `sudo podman`, and the other way round.

**Rootful** (system-wide, like Docker):

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
mkdir -p data && sudo chown 10001 data
sudo podman compose up -d --build
sudo podman compose logs dogubako | grep "Setup token"
```

**Rootless** (as a normal user). The container's user 10001 maps to a
different user on the host, so give the folder to it with `podman unshare`
instead of `sudo chown`:

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
mkdir -p data && podman unshare chown 10001:10001 data
podman compose up -d --build
podman compose logs dogubako | grep "Setup token"
```

`podman compose` hands the compose file to `podman-compose` or
`docker-compose`, whichever is installed. Everything on the Docker page works
with `podman` (or `sudo podman`) in place of `docker`.

## SELinux

The compose file mounts `./data` with `:z`, so it works with SELinux
enforcing. If you add host folders for [folder tools](/self-hosting/docker/#folder-tools),
label them too (`- /srv/media:/media:ro,z`). Never relabel system folders
such as `/home` or `/usr`; mount a subfolder instead.

## Health check

Podman builds images in OCI format by default, and that format has no
`HEALTHCHECK`. The compose file and the Quadlet unit below declare the health
check themselves, so `podman ps` shows `(healthy)` either way. To keep the
image's own health check when you build by hand, use
`podman build --format docker`.

## Start at boot

Podman has no daemon, so `restart: unless-stopped` doesn't bring the
container back after a reboot by itself. Turn on Podman's restart service. It
starts the containers whose restart policy says so, not ones you stopped:

```sh
sudo systemctl enable podman-restart.service                                    # rootful
systemctl --user enable podman-restart.service && loginctl enable-linger "$USER"  # rootless
```

## As a systemd service (Quadlet)

`deploy/dogubako.container` runs the same container as a systemd service
without compose, with the same port, limits, hardening and health check:

```sh
sudo podman build -t dogubako .
sudo cp deploy/dogubako.container /etc/containers/systemd/
sudoedit /etc/containers/systemd/dogubako.container   # set Volume= to your data folder
sudo systemctl daemon-reload
sudo systemctl start dogubako
```

Rootless, build with plain `podman build -t dogubako .`, put the unit in
`~/.config/containers/systemd/`, use `systemctl --user` and run
`loginctl enable-linger "$USER"` once. Quadlet units start at boot
through their `[Install]` section; `systemctl enable` doesn't apply to them.

Settings are `Environment=` lines in the unit. Put secrets such as
`DOGUBAKO_ASSISTANT_API_KEY` in a file referenced by `EnvironmentFile=`, as
plain `KEY=value` lines: unlike `.env`, it doesn't allow comments after a
value.

## Updating

```sh
git pull
sudo podman compose up -d --build      # rootless: without sudo
```

With Quadlet: `git pull`, `sudo podman build -t dogubako .`, then
`sudo systemctl restart dogubako` (rootless: `podman build` and
`systemctl --user restart dogubako`). See
[Operations](/self-hosting/operations/#upgrading) for backups first.
