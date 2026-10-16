# Backend

This directory contains the backend services for the project.

## API Response Caching

The backend uses Redis as a caching layer to reduce RPC calls and improve API response times for frequently accessed data.

### Configuration

Redis caching is configured through environment variables:

| Variable | Description | Default |
| --- | --- | --- |
| `REDIS_URL` | Redis connection URL used by the cache client | `redis://127.0.0.1:6379` |
| `CACHE_METER_TTL_SECONDS` | TTL (in seconds) for cached meter data | `300` (5 minutes) |
| `CACHE_USAGE_HISTORY_TTL_SECONDS` | TTL (in seconds) for cached usage history | `3600` (1 hour) |

### Cached Data

- **Meter data** — cached with a 5-minute TTL (`CACHE_METER_TTL_SECONDS`).
- **Usage history** — cached with a 1-hour TTL (`CACHE_USAGE_HISTORY_TTL_SECONDS`).

### Cache Invalidation

Cached entries are invalidated on mutations so reads never serve stale data:

- **Payments** — payment mutations invalidate the affected meter and usage history cache entries.
- **Usage updates** — usage mutations invalidate the affected meter and usage history cache entries.

### Tests

Caching logic (TTL handling and invalidation on mutations) is covered by the backend test suite.

## Meter Health Monitoring (#834)

Meters publish periodic heartbeats to `solargrid/meters/{meterId}/heartbeat`
(optional JSON payload `{ "responseTimeMs": 120, "error": false }`), or POST to
`/api/meters/:meterId/heartbeat`. The backend tracks the last heartbeat,
average response time, error rate and uptime per meter.

| Status  | Condition                                                        |
|---------|------------------------------------------------------------------|
| 🟢 green  | Heartbeat within the last hour and error rate < 10%            |
| 🟡 yellow | No heartbeat for > 1 hour, or error rate ≥ 10%                 |
| 🔴 red    | No heartbeat for 24 hours — an alert is logged once per outage |

Endpoints:

- `GET /api/meters/health` — dashboard of all meters plus a `{ green, yellow, red }` summary
- `GET /api/meters/:meterId/health` — health of a single meter
- `POST /api/meters/:meterId/heartbeat` — record a heartbeat over HTTP

Tunables: `HEALTH_YELLOW_AFTER_MS`, `HEALTH_RED_AFTER_MS`, `HEARTBEAT_INTERVAL_MS`.

## Automated Credit Scoring (#1003)

A deterministic (non-ML), explainable 0-1000 credit score per Stellar
address, built from two existing data sources rather than a new ledger:

- **Activity** — `lib/revenueStore.ts` (#1002): count and lifetime volume of
  successful payments by that address.
- **Delays / defaults** — `lib/billing.ts` (#902) bills, joined to an
  address via the billing account's `stellar_address` field (the closest
  existing "which meters does this user pay for" mapping). Each bill is
  classified against its `due_at`:
  - paid at/before `due_at` → **on-time**
  - paid after `due_at`, or unpaid and past `due_at` → **late**
  - unpaid and past `due_at + CREDIT_DEFAULT_GRACE_DAYS` (default 30 days) → **default**
  - unpaid but not yet due → ignored (nothing to judge yet)

### Scoring formula

```
score = 300                                   (base)
      + 500 × reliabilityRatio                (on-time share of judged bills; 0.5 if no bills yet)
      + 150 × min(1, lifetimeVolumeXLM / 500)  (volume factor, caps at 500 XLM paid)
      + 100 × min(1, accountAgeDays / 365)     (tenure factor, caps at 1 year)
      -  15 × lateCount
      -  60 × defaultCount
      clamped to [0, 1000]
```

### Tiers

| Score range | Tier       |
|-------------|------------|
| 850-1000    | Excellent  |
| 700-849     | Good       |
| 500-699     | Fair       |
| 300-499     | Poor       |
| 0-299       | Very Poor  |

### Payment plan recommendation & credit limit

| Score     | Recommended plan |
|-----------|-------------------|
| 0-499     | Daily             |
| 500-749   | Weekly            |
| 750-1000  | Monthly           |

Credit limit scales linearly with score up to a 5,000 XLM cap:
`creditLimitStroops = round(score / 1000 × 50,000,000,000)`.

### Endpoints

- `GET /api/users/:address/credit-score` — computed score, tier, factors,
  recommended payment plan and credit limit, plus any active admin override.
- `POST /api/users/:address/credit-score/override` — admin only
  (`X-Admin-Key` or a bearer admin JWT). Body: `{ score?, credit_limit_stroops?, reason, expires_at? }`.
  At least one of `score` / `credit_limit_stroops` is required. While active
  (not expired), the override's score/limit take precedence over the
  computed values in the GET response; the computed result is still returned
  under `computed` for comparison.
- `DELETE /api/users/:address/credit-score/override` — admin only; clears a
  stored override.

### Tests

The scoring formula, tier thresholds, payment-plan/credit-limit thresholds,
and admin-override precedence/expiry are covered by
`src/lib/creditScoring.test.ts`.
