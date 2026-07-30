# SpecForge Deployment & Hosting

How the specforge.deepwhaleai.com landing and doc.specforge.deepwhaleai.com docs
site are built, deployed, and served. Read this before touching any of the moving
parts below.

---

## 1. Topology at a glance

```
                                 Public DNS
                                       │
                  ┌────────────────────┼────────────────────────────┐
                  ▼                    ▼                            ▼
   deepwhaleai.com / www      specforge.deepwhaleai.com    doc.specforge.deepwhaleai.com
                  │                    │                            │
                  └────────────────────┴──────────┬─────────────────┘
                                                 │  74.50.113.132
                                                 ▼
                  ┌──────────────────────────────────────────────┐
                  │  Traefik ingress DaemonSet (ingress/traefik)  │
                  │  hostPort 80 → containerPort 8000            │
                  │  hostPort 443 → containerPort 8443           │
                  │  TLS terminated, cert-manager issued         │
                  └────────┬──────────────────┬───────────────────┘
                           │                  │
                ┌──────────▼────────┐  ┌──────▼─────────────────┐
                │ k8s Service       │  │ k8s Service            │
                │ specforge-host    │  │ specforge-docs         │
                │ ClusterIP         │  │ ClusterIP              │
                │ Selector: <none>  │  │ Selector: <none>       │
                │ Endpoints:        │  │ Endpoints:             │
                │   74.50.113.132   │  │   10.0.1.1             │
                │     :8082         │  │     :8083 (targetPort) │
                └──────────┬────────┘  └──────┬─────────────────┘
                           │                  │
                  ┌────────▼────────┐  ┌──────▼─────────────────┐
                  │ Host nginx      │  │ Host nginx             │
                  │ :8082 →         │  │ :8083 →                │
                  │ /var/www/       │  │ /var/www/              │
                  │   specforge/    │  │   specforge/docs/      │
                  │   index.html    │  │   (Docusaurus build)   │
                  └─────────────────┘  └────────────────────────┘
```

Key facts:

- **Traefik owns public ingress.** It binds 80/443 on the host via pod
  `hostPort` and CNI DNAT rules. It terminates TLS using certs issued by
  cert-manager (ClusterIssuer `letsencrypt-prod`).
- **Host nginx is a static-only origin.** It listens only on
  `127.0.0.1:8082` (and `[::]:8082`) for the landing and `127.0.0.1:8083`
  (and `[::]:8083`) for the docs. It must never bind 80/443 — that would
  conflict with Traefik.
- **k8s Services are selectorless.** They bypass the endpoint controller and
  point at the host's IPs directly so Traefik reaches the host's nginx
  without a hop.
- **cert-manager** issues and rotates the TLS certs into secrets named
  `<host-with-dashes>-tls` (e.g. `specforge-deepwhaleai-com-tls`,
  `doc-specforge-deepwhaleai-com-tls`).

## 2. File & resource map

| Layer | Path / object |
|---|---|
| Source landing (edit here) | `website/index.html` |
| Built docs source | `docs-site/docs/`, `docs-site/docusaurus.config.ts` |
| Docs build output | `docs-site/build/` (auto-emits `CNAME`) |
| Served landing (origin) | `/var/www/specforge/index.html` |
| Served docs (origin) | `/var/www/specforge/docs/` (includes `CNAME`) |
| Host nginx vhost | `/etc/nginx/sites-available/specforge` (symlink from `sites-enabled/`) |
| Traefik ingress | `default/specforge-http`, `default/specforge-https`, `default/specforge-docs-http`, `default/specforge-docs-https` |
| Traefik backend Service | `default/specforge-host` (targetPort 8082), `default/specforge-docs` (targetPort 8083) |
| Traefik middleware | `default/redirect-https` (HTTP → HTTPS) |
| Cert-manager ClusterIssuer | `default/letsencrypt-prod` |
| Cert-manager Certificate | `default/specforge-deepwhaleai-com`, `default/doc-specforge-deepwhaleai-com` |
| TLS secret | `default/specforge-deepwhaleai-com-tls`, `default/doc-specforge-deepwhaleai-com-tls` |

## 3. Update workflows

### Landing page

1. Edit `website/index.html` in this repo.
2. `cp website/index.html /var/www/specforge/index.html`
3. `nginx -s reload`
4. Commit + push.

### Docs site

1. Edit content in `docs-site/docs/` (or `docs-site/docusaurus.config.ts`).
2. `cd docs-site && npm run build`
3. `rm -rf /var/www/specforge/docs && cp -a build /var/www/specforge/docs`
   (the build emits `build/CNAME` automatically — verify it
   still contains `doc.specforge.deepwhaleai.com`).
4. `nginx -s reload`
5. Commit + push.

### TLS renewal

Automatic (cert-manager). To force:

```bash
microk8s kubectl cert-manager renew doc-specforge-deepwhaleai-com
```

### New subdomain

1. Add a CNAME in Namecheap pointing to `specforge.deepwhaleai.com.`
   (apex already resolves to `74.50.113.132`, so the new subdomain
   inherits that).
2. Create Ingresses (HTTP + HTTPS) and a Certificate, then annotate the
   HTTP ingress with `traefik.ingress.kubernetes.io/router.middlewares:
   redirect-https@kubernetescrd`.
3. If the origin is a static file served by host nginx, also add a
   selectorless Service pointing at the host's IP and create both an
   Endpoints and an EndpointSlice (the auto-mirroring controller does not
   generate slices for selectorless Services — see §5 below).

## 4. Quick verification checklist

```bash
# Landing served by host nginx (origin)
curl -H "Host: specforge.deepwhaleai.com" http://127.0.0.1:8082/

# Docs served by host nginx (origin)
curl -H "Host: doc.specforge.deepwhaleai.com" http://127.0.0.1:8083/

# Through Traefik (cluster IPs of the selectorless Services)
curl -H "Host: specforge.deepwhaleai.com" http://10.152.183.130/
curl -H "Host: doc.specforge.deepwhaleai.com" http://10.152.183.227/

# TLS
microk8s kubectl get certificates -A
microk8s kubectl get ingress -A
nginx -t
```

## 5. Lessons — what NOT to do

These mistakes were made and unwound during the original bring-up. They
are the source of the duplicate-edge / stale-endpoint / missing-banner
problems documented in CHANGELOG.md.

- **Do not bind port 80 (or 443) on the host nginx.** Traefik owns the
  public edge via CNI DNAT. A second listener on port 80 intercepts
  packets (or refuses them after a CNI race) and silently breaks HTTPS
  even when Traefik appears healthy. Host nginx must serve only on
  high ports (8082/8083).

- **Do not let the served landing diverge from the repo copy.** Edit
  the repo, copy to `/var/www/specforge/`, commit. The repo is the
  source of truth; the served file is a build artifact.

- **Do not reference a Traefik Middleware that doesn't exist.** Several
  Ingresses originally annotated
  `traefik.ingress.kubernetes.io/router.middlewares:
  default-redirect-https@kubernetescrd` while no such Middleware was
  defined in the cluster. Traefik silently dropped the redirect. Always
  `kubectl apply -f` the Middleware first, then annotate.

- **Do not leave an EndpointSlice (or legacy Endpoints) pointing at a
  dead pod IP.** Selectorless Services retain whatever address you put
  in their endpoint object, and kube-proxy will faithfully program iptables
  to DNAT to it. When the backing pod is gone, the Service silently
  503s for every caller.

- **Do not rely on the Endpoints → EndpointSlice mirroring controller
  for selectorless Services.** The controller only generates slices
  for Services with a selector. For selectorless Services you must
  create both `Endpoints` and `EndpointSlice` manually. (If
  kube-proxy's iptables don't reconcile promptly — e.g. after a long
  uptime with kubelite — you may need to manually rewrite the
  `KUBE-SEP-*` chain rule via
  `iptables-legacy -t nat -R KUBE-SEP-<hash> 2 ... -j DNAT
  --to-destination <ip>:<port>`. This is a workaround for a stale
  kubelite; restarting kubelite resolves it cleanly.)

- **Do not bake a hostname into a Docusaurus `url`/`baseUrl` and forget
  the static-files `CNAME`.** Both must agree. The config tells Docusaurus
  what absolute URL to emit; the `static/CNAME` (copied to `build/CNAME`
  and on to the served `/var/www/specforge/docs/CNAME`) is what GitHub
  Pages / cert-manager's HTTP-01 challenge reads. Mismatches cause the
  "Your Docusaurus site did not load properly" base-URL banner to appear.

- **Do not enable TLS 1.0 / 1.1.** Nginx defaults to `TLSv1 TLSv1.1
  TLSv1.2 TLSv1.3` in `nginx.conf`. Keep `ssl_protocols TLSv1.2 TLSv1.3;`
  only.

- **Do not let UFW rules accumulate dead port allowances.** When a
  service moves (e.g. Traefik NodePorts 32689 → 30089/31116), update UFW
  in the same change. Stale allow rules hide real exposure and confuse
  audits.

## 6. Bring-up journal (one-time)

The original setup required unwinding several misconfigurations in this
order. If you're starting fresh, follow these steps rather than the
historical sequence:

1. **DNS:** in Namecheap, apex `deepwhaleai.com` → A `74.50.113.132`,
   `specforge` → CNAME `specforge.deepwhaleai.com.`,
   `doc.specforge` → CNAME `specforge.deepwhaleai.com.`
2. **Cluster:** install microk8s; enable `dns`, `ingress`, `cert-manager`,
   `hostpath-storage`, `metallb`.
3. **Cert-manager ClusterIssuer:** `letsencrypt-prod` (already present
   in this cluster).
4. **Host nginx origin:** create `/var/www/specforge/{index.html,docs/}`
   and `/etc/nginx/sites-available/specforge` listening on 8082/8083.
   Enable via symlink. **Do not enable any site listening on 80.**
5. **k8s Services:** `specforge-host` (targetPort 8082) and
   `specforge-docs` (targetPort 8083), both selectorless. Manually
   create Endpoints (e.g. `10.0.1.1:80` mapped via the SEP rule to the
   host nginx port) and EndpointSlice.
6. **Traefik Middleware:** `redirect-https` in `default`.
7. **Ingresses:** `specforge-http`, `specforge-https`,
   `specforge-docs-http`, `specforge-docs-https`. HTTP ingresses get
   `traefik.ingress.kubernetes.io/router.middlewares:
   redirect-https@kubernetescrd`.
8. **Certificates:** one per hostname, referencing `letsencrypt-prod`.
9. **UFW:** allow 22, 80, 443, 30089, 31116 (current Traefik NodePorts).
10. **Verify** with the §4 checklist.

## 7. Known issues / future work

- **`cert-manager-webhook` Service has no endpoints** (legacy selector
  mismatch with the running pod). Cert issuance currently works
  because cert-manager-controller uses a local client, but renewal
  through the webhook could break. Fix the selector or recreate the
  Service.
- **Traefik OOM was seen once** (~77 min uptime before audit) — host
  memory pressure. Consider raising kubelet memory limits or moving
  Traefik to a dedicated node.
- **`kubectl logs` against the kubelet fails** with `x509: certificate
  is valid for 74.50.113.132, 2604:4500:9:..., not 10.0.1.1`. The
  apiserver reaches kubelet at `10.0.1.1` but the kubelet cert doesn't
  include that SAN. Recycle the kubelet serving cert or add `10.0.1.1`
  to its SAN list.
- **Live landing lacks real social proof** (logos, testimonials, GitHub
  stars). The current "Battle-Tested at Scale" section is parser
  capacity, not adoption. Replace once real users exist.