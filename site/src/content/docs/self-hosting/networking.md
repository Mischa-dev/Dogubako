---
title: Networking
description: Reach your Dogubako server on your LAN, over Tailscale, or behind a reverse proxy with HTTPS.
sidebar:
  order: 4
---

By default the server listens on `127.0.0.1:8420` and is reachable only from
the machine it runs on. Open it up in whichever of these ways suits you.
Accounts are required in every case.

## On your LAN

With Docker, change the port mapping in `docker-compose.yml`:

```yaml
ports:
  - "8420:8420"
```

Natively, set `DOGUBAKO_BIND=0.0.0.0:8420`. Then browse to
`http://<server-ip>:8420`.

Plain HTTP on a LAN works, but anyone on the network can read the traffic,
including session cookies. Prefer HTTPS through Tailscale or a reverse proxy,
then set:

```sh
DOGUBAKO_SECURE_COOKIES=true
DOGUBAKO_PUBLIC_URL=https://tools.example.com
```

## Tailscale

Tailscale gives you HTTPS on your private tailnet with no ports opened to the
internet.

**With the bundled sidecar.** It joins your tailnet as `dogubako` and serves
HTTPS with `tailscale serve`:

```sh
TS_AUTHKEY=tskey-auth-… docker compose \
  -f docker-compose.yml -f deploy/docker-compose.tailscale.yml up -d
```

Open `https://dogubako.<your-tailnet>.ts.net`. The overlay turns on secure
cookies and removes the host port mapping, so the server is reachable only
through the tailnet.

**With Tailscale on the host.** Keep the default `127.0.0.1` binding and run:

```sh
tailscale serve --bg 8420
```

That takes the machine's HTTPS port 443. If `tailscale serve status` shows
something there already, Tailscale replaces it without asking, so use another
port instead and open `https://<machine>.<tailnet>.ts.net:8443`:

```sh
tailscale serve --bg --https=8443 8420
```

Set `DOGUBAKO_PUBLIC_URL` to the address you end up with, port included.

Tailscale ACLs make a good outer layer: for example, only your devices and
your agents' machines may reach the server. Dogubako's accounts and agent
policies still apply inside.

## Reverse proxy

Any reverse proxy works. Two things matter:

1. **Large uploads.** Raise the proxy's body size limit to at least
   `DOGUBAKO_MAX_UPLOAD_MB` (2 GB by default).
2. **Streaming.** Job progress (`/api/v1/jobs/*/events`) and MCP (`/mcp`) use
   server-sent events. Turn off response buffering for those paths if your
   proxy buffers.

Then set `DOGUBAKO_PUBLIC_URL` to the external URL and
`DOGUBAKO_SECURE_COOKIES=true`.

### Caddy

```
tools.example.com {
    reverse_proxy 127.0.0.1:8420
    request_body {
        max_size 2GB
    }
}
```

Caddy streams server-sent events without extra configuration.

### nginx

```nginx
server {
    listen 443 ssl http2;
    server_name tools.example.com;
    # ssl_certificate …; ssl_certificate_key …;

    client_max_body_size 2g;

    location / {
        proxy_pass http://127.0.0.1:8420;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_buffering off;          # progress events and MCP streams
        proxy_read_timeout 1h;        # long jobs
        proxy_request_buffering off;  # stream large uploads
    }
}
```

## Browser-based MCP clients

The `/mcp` endpoint validates the `Origin` header to block DNS-rebinding
attacks. If an MCP client runs in a browser on another origin, add that
origin to `DOGUBAKO_ALLOWED_ORIGINS` (comma-separated).

## Network tools and private addresses

Tools that make network requests (DNS, HTTP headers, TLS inspection and so
on) refuse private, loopback and link-local addresses on a server by default,
so nobody can use your server to probe your home network. Set
`DOGUBAKO_ALLOW_PRIVATE_NETWORK=true` only if every account on the server is
allowed to reach your internal network.
