# FlareSync Workflow

This document describes FlareSync’s runtime behavior (control flow, external calls, retries, and side effects) as implemented in the current codebase.

## High-Level Flow

```mermaid
flowchart TD
  A["Process start"] --> B["Init logging (LOG_CONFIG_PATH or log4rs.yaml)"]
  B --> C["Load config from env/.env"]
  C --> D["Build reqwest Client (30s timeout)"]
  D --> E["Loop forever"]

  E --> F["Get current public IPv4"]
  F -->|quorum ok| G["For each DOMAIN_NAME"]
  F -->|error| F1["Log error; sleep 60s; continue loop"]
  F1 --> E

  G --> H["Fetch Cloudflare A record for domain"]
  H -->|record found| I["Compare record.content vs current IPv4"]
  H -->|no record| H1["Warn; continue next domain"]
  H1 --> G

  I -->|same| I1["Log no update needed"]
  I -->|changed| J["Backup existing DNS record to ./backups"]
  J --> K["Update Cloudflare A record to current IPv4"]
  K --> K1["Log update success"]

  I1 --> G
  K1 --> G

  G --> L["Sleep UPDATE_INTERVAL"]
  L --> E
```

## Inputs and Configuration

```mermaid
flowchart LR
  ENV["Environment + .env"] --> CFG["Config::from_env()"]
  CFG --> TOK["CLOUDFLARE_API_TOKEN (required)"]
  CFG --> ZID["CLOUDFLARE_ZONE_ID (required)"]
  CFG --> DOM["DOMAIN_NAME (required; comma/semicolon-separated; empty entries ignored)"]
  CFG --> INT["UPDATE_INTERVAL minutes (required; must be >= 1)"]
  ENV --> LOG["LOG_CONFIG_PATH (optional)"]
```

- `LOG_CONFIG_PATH` defaults to `log4rs.yaml` if unset.
- `DOMAIN_NAME` may contain multiple entries separated by `,` or `;`. Empty entries are dropped; if all entries are empty, startup fails.
- `UPDATE_INTERVAL` is interpreted as minutes and must be `>= 1`.

## Public IP Discovery (Multi-Source + Quorum)

FlareSync queries multiple public-IP endpoints concurrently and requires agreement by quorum to accept a result.

**Sources (hardcoded)**
- `https://api.ipify.org`
- `https://checkip.amazonaws.com`
- `https://ipv4.icanhazip.com`

**Policy**
- Fetch all three in parallel.
- Accept the IPv4 address only if at least **2 out of 3** sources return the same value.
- If quorum fails, treat as an error and retry later.

```mermaid
sequenceDiagram
  autonumber
  participant App as FlareSync
  participant S1 as ipify.org
  participant S2 as checkip.amazonaws.com
  participant S3 as icanhazip.com

  App->>S1: GET / (10s timeout, retries with backoff)
  App->>S2: GET / (10s timeout, retries with backoff)
  App->>S3: GET / (10s timeout, retries with backoff)

  alt 2-of-3 agree on IPv4
    S1-->>App: "203.0.113.10"
    S2-->>App: "203.0.113.10"
    S3-->>App: "203.0.113.11"
    App-->>App: quorum satisfied (203.0.113.10)
  else quorum not satisfied (or too many failures)
    S1-->>App: timeout or error
    S2-->>App: "203.0.113.10"
    S3-->>App: "203.0.113.11"
    App-->>App: error (no quorum)
    App-->>App: sleep 60s and retry loop
  end
```

**Retry behavior per source**
- Per-attempt timeout: 10 seconds (request and response body).
- Retry up to 3 times with exponential backoff (starting at 1s, doubling, capped).
- Retries trigger on network failures and explicit timeout errors.

## Cloudflare DNS Check/Update

For each configured domain, FlareSync:
1. Fetches the existing **A** record matching that exact name in the given Zone.
2. If a record exists, compares current record IP with current public IPv4.
3. If different, backs up the record JSON to `./backups/` and updates the record via Cloudflare API.
4. If the record is missing, it logs a warning and does not create records.

```mermaid
sequenceDiagram
  autonumber
  participant App as FlareSync
  participant CF as Cloudflare API
  participant FS as Local filesystem

  App->>CF: GET /zones/{zone}/dns_records?type=A&name={domain}
  alt record exists
    CF-->>App: success=true, result=[DnsRecord]
    App-->>App: compare record.content vs current IPv4
    alt IP changed
      App->>FS: write ./backups/{timestamp}_{sanitized-name}_backup.json
      App->>CF: PUT /zones/{zone}/dns_records/{id} (content=current IPv4)
      CF-->>App: success=true
    else IP unchanged
      App-->>App: no update
    end
  else record missing
    CF-->>App: success=true, result=[]
    App-->>App: warn "No matching DNS record found"
  end
```

### URL Encoding

The Cloudflare DNS-record lookup uses a structured query builder (not string concatenation), so the `name=` parameter is URL-encoded correctly for edge cases (e.g., wildcard names like `*.example.com`).

### Backups (Side Effects)

When an update occurs:
- `./backups/` is created if missing.
- The existing DNS record is saved as pretty-printed JSON before the update.
- The filename uses a sanitized version of the record name to avoid unsafe filesystem characters:
  - Allowed: ASCII letters/digits plus `.`, `_`, `-`
  - All other characters become `_`
  - Component is length-capped

## Retry & Error Handling

```mermaid
stateDiagram-v2
  [*] --> Startup
  Startup --> Running: config ok
  Startup --> [*]: config error

  Running --> ResolveIP
  ResolveIP --> ResolveIP_Wait: error (no quorum or failures)
  ResolveIP_Wait --> Running: after 60s
  ResolveIP --> UpdateDomains: success (IPv4)

  UpdateDomains --> PerDomain
  state PerDomain {
    [*] --> FetchRecord
    FetchRecord --> NoRecord: empty result
    FetchRecord --> Compare: record found
    Compare --> NoChange: IP same
    Compare --> Backup: IP changed
    Backup --> UpdateRecord
    UpdateRecord --> Done
    NoRecord --> Done
    NoChange --> Done
  }

  UpdateDomains --> SleepInterval
  SleepInterval --> Running: after UPDATE_INTERVAL
```

### Cloudflare retries

Cloudflare requests use bounded exponential backoff retries for transient failures:
- Network/HTTP transient: request-level failures, HTTP `429`, HTTP `5xx`.
- API-level transient: HTTP `200` with `success=false` where `errors` look transient (e.g., Cloudflare code `1015` or messages suggesting rate limiting / temporary issues).

Non-transient Cloudflare API errors fail fast for that domain and FlareSync continues with the next domain.

## Logging

Logging is initialized from:
- `LOG_CONFIG_PATH` if set (Docker image sets this to `log4rs.docker.yaml`)
- Otherwise `log4rs.yaml`

The app logs:
- Startup
- Current public IP
- Per-domain decisions (no record / no change / updated)
- Retry warnings and errors

## Deployment Notes (Docker)

```mermaid
flowchart LR
  IMG["Container image"] --> BIN["flaresync binary"]
  IMG --> LC["LOG_CONFIG_PATH=log4rs.docker.yaml"]
  VOL1["./backups"] -->|mounted to| APPBK["/app/backups"]
```

- The container image sets `LOG_CONFIG_PATH=log4rs.docker.yaml` to log to stdout (useful for `docker logs`).
- Backups are typically volume-mounted so they persist across container restarts.
