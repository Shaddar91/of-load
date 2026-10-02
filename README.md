# of-load

Stress API for testing pod and node autoscaling. After a user signs in on of-web, the page shows Easy, Medium and High buttons. A press makes the browser keep that level's number of requests in flight against of-load until the level's time runs out, the user presses Stop, or the tab closes. Each request burns CPU and holds memory, so the pods' CPU and memory climb, the Helm chart's HorizontalPodAutoscaler (HPA) adds pods, and Karpenter adds nodes when the new pods no longer fit.

of-load accepts the bearer token of-api issues at login and checks it with of-api's `GET /api/v1/me`. It is written in Rust (axum on tokio), builds into one static binary, and writes one JSON log line per request to stdout.

## Levels

of-load answers single requests; the caller drives a level. A level keeps `concurrency` requests in flight for `duration_seconds`, and each request busy-loops one CPU core for `cpu_ms` while it holds `mem_mib` MiB.

| Level | Local: `levels.json` | Cluster: the chart's `levels` |
|---|---|---|
| easy | 1 in flight for 30 s, 200 ms and 32 MiB each | 1 for 300 s, 500 ms and 64 MiB each |
| medium | 2 for 30 s, 400 ms and 64 MiB each | 4 for 300 s, 1000 ms and 128 MiB each |
| high | 4 for 30 s, 800 ms and 128 MiB each | 12 for 300 s, 1500 ms and 192 MiB each |

Locally, high fills the 2-CPU cap and holds about 0.5 GiB. In the cluster each pod requests 1 CPU and the HPA aims for 70 % of it, so high's 12 busy cores call for about 18 pods (12 ÷ 0.7, rounded up), between the floor of 2 and the ceiling of 20. Medium calls for about 6; easy stays at 2.

## API

| Request | Answer |
|---|---|
| `GET /healthz` | 200 `{"status":"ok"}`, no token needed |
| `GET /api/v1/stress` with `Authorization: Bearer <token>` | 200 `{"levels":[…]}`: easy, medium and high in that order, each with `name`, `concurrency`, `duration_seconds`, `cpu_ms` and `mem_mib` |
| `POST /api/v1/stress/<level>` with the bearer | holds `mem_mib` MiB with every 4 KiB page written while a blocking thread busy-loops for `cpu_ms` of wall clock, frees the memory, then 200 `{"level","pod","cpu_ms","mem_mib","elapsed_ms"}` |

Errors come back as `{"error":"…"}`:

| Status | `error` | When |
|---|---|---|
| 401 | `invalid token` | the Authorization header is missing or malformed, or of-api answers 401 |
| 404 | `unknown level` | the level is not easy, medium or high |
| 429 | `busy` | the process already runs `STRESS_MAX_INFLIGHT` burns; of-web retries after 250 ms |
| 503 | `auth unavailable` | of-api is unreachable, silent for 2 s, or answers anything but 200 or 401 |
| 500 | `burn failed` | the burn thread panicked |

For the token check, of-load sends `GET {OF_API_URL}/api/v1/me` with the same bearer over plain HTTP. A 200 is cached in memory under the token's SHA-256 for `AUTH_CACHE_SECONDS`, never past the `expires_at` of-api returned; a 401 is not cached. Neither the token nor the Authorization header is logged.

CORS allows the origins in `CORS_ALLOWED_ORIGINS`, the methods GET, POST and OPTIONS, and the headers Authorization and Content-Type, and lets the browser cache a preflight for 600 s.

## Configuration

| Variable | Default | Use |
|---|---|---|
| `PORT` | `8000` | port to listen on, all interfaces |
| `OF_API_URL` | `http://api:8000` | of-api base URL for the token check; HTTP only, the binary has no TLS |
| `CORS_ALLOWED_ORIGINS` | `http://localhost:5173` | comma-separated browser origins |
| `STRESS_LEVELS_FILE` | unset: the `levels.json` built into the binary | path to a levels file of the same shape |
| `STRESS_MAX_INFLIGHT` | `4` | burns one process runs at once; past that it answers 429 |
| `STRESS_MAX_CPU_MS` | `2000` | ceiling for any level's `cpu_ms` |
| `STRESS_MAX_MEM_MIB` | `256` | ceiling for any level's `mem_mib` |
| `AUTH_CACHE_SECONDS` | `30` | how long an accepted token skips the of-api call |
| `POD_NAME` | `HOSTNAME`, else `unknown` | the `pod` in each answer; the chart sets it to the pod's name |
| `RUST_LOG` | `info` | log filter |

At startup each level is clamped: `cpu_ms` and `mem_mib` to the two ceilings, `concurrency` to 1–64, `duration_seconds` to 1–900. Startup fails, with a JSON log line giving the reason, unless the levels file holds easy, medium and high. Other level names are ignored.

## Run it locally

`compose.yaml` caps the `load` container at 2 CPUs, 1 GiB of memory and 128 processes, and publishes it on `127.0.0.1:${OF_LOAD_PORT:-8001}`. Outside Compose, keep the same limits: `docker run --cpus 2 --memory 1g --pids-limit 128 …`.

The token check needs of-api, so run of-load in the dev lane: `../dev-lane/compose.yaml`, in a folder beside the of-api, of-web and of-load checkouts and outside every repo. It includes this repo's `compose.yaml` and of-api's (Postgres and the API) and adds the of-web dev server. From that folder, with the ports moved off 5432, 8000 and 8001:

```sh
env UID="$(id -u)" GID="$(id -g)" OF_DB_PORT=55432 OF_API_PORT=18000 OF_LOAD_PORT=18001 \
  docker compose up -d --build
```

`env` is there because bash will not assign `UID`. The web stays on `http://localhost:5173`, the origin both APIs accept, and follows the two API ports. Sign in as `demo` with the `OF_API_USER_PASSWORD` value from of-api's `compose.yaml` and press a level, or drive one with curl:

```sh
TOKEN=$(curl -s localhost:18000/api/v1/login -H 'content-type: application/json' \
  -d '{"username":"demo","password":"<OF_API_USER_PASSWORD from of-api compose.yaml>"}' | jq -r .token)
curl -s localhost:18001/api/v1/stress -H "Authorization: Bearer $TOKEN"
curl -s -X POST localhost:18001/api/v1/stress/easy -H "Authorization: Bearer $TOKEN"
```

One POST is one burn. To hold a level the way the page does, keep its `concurrency` requests going for its `duration_seconds`. For local high, that is 4 for 30 s:

```sh
end=$((SECONDS + 30))
for _ in 1 2 3 4; do
  while [ "$SECONDS" -lt "$end" ]; do
    curl -s -o /dev/null -X POST localhost:18001/api/v1/stress/high -H "Authorization: Bearer $TOKEN" || sleep 1
  done &
done
wait
```

Watch it from a second shell in the dev-lane folder:

```sh
docker stats $(docker compose ps -q load)
```

CPU runs about 100 % per request in flight, up to the 200 % cap, and memory about `mem_mib` per request in flight: near 33 MiB on easy, 130 MiB on medium and 515 MiB on high. A single sample can read a few percent over 200 %, because `docker stats` reads once a second while the kernel enforces the quota per 100 ms period. `docker inspect -f '{{.HostConfig.NanoCpus}} {{.HostConfig.Memory}}' $(docker compose ps -q load)` prints `2000000000 1073741824`, the cap itself.

`docker compose down -v` in the dev-lane folder removes the containers and the lane's database.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`rust-toolchain.toml` pins Rust 1.96.0 with rustfmt and clippy; rustup installs it on the first `cargo` call. CI runs the same three before it builds an image.

## Image

The Dockerfile compiles a static musl binary on `rust:1.96-alpine` and copies it into `gcr.io/distroless/static-debian12:nonroot`: no shell, uid 65532, port 8000. It passes no `--target`, so each platform's build stage compiles for its own architecture and one Dockerfile serves amd64 and arm64. The `CARGO_BUILD_JOBS` build argument (default 4) bounds the compile's parallel jobs.

`docker build -t of-load .` builds for this machine. A local two-architecture build needs a builder that handles several platforms, such as buildx on the `docker-container` driver, plus QEMU for the other architecture, and it compiles slowly under emulation. CI does not emulate: it builds amd64 on `ubuntu-24.04` and arm64 on `ubuntu-24.04-arm`, checks `/healthz` in each image, pushes `<sha>-amd64` and `<sha>-arm64`, and joins them under one tag, `<sha>`, from which a node of either architecture pulls its own image. The ECR repository's tags are immutable, so CI skips a tag that already exists.

## How it ships

Terraform outside this repo owns the cloud side: an ECR stack, an app-service stack and the frontend stack. Nothing here applies it.

1. Push this repo's `master` before the ECR stack's first apply. That apply commits `.github/workflows/ci.yml` here, and the commit starts a CI run; on a repo without the code that run fails, and it needs a re-run after the push.
2. The ECR stack creates the `of-load` ECR repository, the CI role GitHub Actions assumes over OIDC from `master`, the Actions secrets `AWS_ROLE_ARN`, `AWS_REGION` and `ECR_REPOSITORY`, and the workflow. Edit the workflow in that stack; its next apply overwrites a change made here.
3. Every push and pull request to `master` runs the checks and builds both images. Only `master` pushes them.
4. Before the app-service stack's first apply, set the three values in its `helm/values.yaml` that ship as `CHANGEME`: `image.repository` (the ECR repository URL), `image.tag` (a `<sha>` CI pushed) and `cors.allowedOrigins` (the origin the deployed of-web is served from). Left as `CHANGEME`, the pods cannot pull their image and the browser's calls fail CORS.
5. The app-service stack creates the target group, a host rule for `load.<zone>` on the HTTPS listener, a certificate and a DNS record for that name, and the Argo CD Application. On apply it commits the chart to the of-helm repo under `charts/of-load`, and Argo CD deploys it to namespace `of-load` on arm64 nodes. The Argo CD project has to allow namespace `of-load`, or it refuses the Application.
6. The frontend stack passes `https://load.<zone>` to of-web's build as `VITE_LOAD_API_BASE_URL`.

## Scaling test in the cluster

With the app service synced and of-web deployed, sign in and press High. Watch from three shells:

```sh
kubectl -n of-load get hpa -w
kubectl get nodeclaims -w
kubectl -n of-load get pods -o wide -w
```

- The HPA's CPU reading jumps to about 100 % against its 70 % target, and the replica count climbs from 2 in steps toward about 18.
- Pods that do not fit on the running nodes stay Pending. Karpenter creates a NodeClaim (its record of a node it is launching) for them, the new arm64 nodes join, and the pods start there.
- The page lists each new pod as it answers. While pods are short, the page's busy count grows: each pod runs 4 burns at a time and answers 429 to the rest.
- When the run ends (300 s or Stop), CPU and memory fall. After its five-minute scale-down window the HPA returns to 2 pods, and Karpenter removes the emptied nodes.

If the HPA's targets read `<unknown>`, metrics-server is not running; `kubectl -n of-load describe hpa` shows both metrics and the events. If pods grow but no node comes, they still fit on the running nodes: raise `concurrency` on the high entry of `levels`, or `autoscaling.maxReplicas`, in the chart values. The NodePool's CPU limit caps how many nodes Karpenter adds.
