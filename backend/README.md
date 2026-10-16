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

## Energy Storage Battery Integration (#980)

Meters with an attached battery publish telemetry to
`solargrid/meters/{meterId}/battery` (or POST to
`/api/meters/:meterId/battery`). The JSON payload reports the raw battery
measurements:

```json
{
  "voltage": 51.2,
  "current": 12.5,
  "stateOfCharge": 87.5
}
```

| Field           | Description                                              |
|-----------------|----------------------------------------------------------|
| `voltage`       | Battery voltage in volts                                 |
| `current`       | Battery current in amps (positive = charging)            |
| `stateOfCharge` | State of charge as a percentage (0–100)                  |

### Battery Analytics

The backend derives health and cycle information from the reported
telemetry:

- **State of charge (SOC)** — the latest reported charge level, expressed as a
  percentage of capacity.
- **Capacity** — the usable capacity in kWh, tracked per meter.
- **Health** — the ratio of current usable capacity to the original rated
  capacity, expressed as a percentage. A new battery reports 100% health.
- **Cycles** — the number of equivalent full charge/discharge cycles,
  accumulated from the charge throughput reported by the meter.

### Battery Endpoint

- `GET /api/meters/:meterId/battery` — returns the battery stats for a meter:
  charge level (SOC), capacity, health percentage, cycle count, and the
  predicted replacement date.

### Degradation Alert

When battery health degrades below **80%**, the backend logs a degradation
alert for the meter so operators can plan a replacement.

### Replacement Date Prediction

The backend extrapolates the observed health degradation rate over time to
predict a **battery replacement date** — the date at which health is expected
to fall below the replacement threshold. The prediction is included in the
`GET /api/meters/:meterId/battery` response.

### Tests

Battery monitoring (telemetry ingestion, health/cycle calculation, the
below-80% degradation alert, and replacement date prediction) is covered by
the backend test suite.
