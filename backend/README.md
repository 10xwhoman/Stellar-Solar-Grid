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

## Appliance-Level Energy Monitoring (#983)

The backend disaggregates whole-meter consumption into per-appliance usage so
users can see which devices consume the most energy.

### Disaggregation Approach

Appliances are identified from power signatures using ML-based clustering:

- High-frequency power samples are windowed and reduced to features
  (steady-state power draw, inrush/startup transient, duty cycle and
  on/off edge shape).
- Feature vectors are clustered (k-means style) into power signatures; each
  cluster is matched against a signature library to label the appliance
  (e.g. refrigerator, HVAC, water heater, EV charger).
- Unmatched clusters are stored as `unknown` and can be labelled later.
- Sub-metered devices (IoT plugs) take precedence over ML inference when a
  sub-meter is associated with the meter.

### `appliance_usage` Table

| Column | Type | Description |
| --- | --- | --- |
| `id` | uuid | Primary key |
| `meter_id` | uuid | Meter the usage belongs to |
| `appliance_id` | uuid | Identified appliance (nullable for `unknown`) |
| `appliance_type` | text | Label, e.g. `refrigerator`, `hvac`, `unknown` |
| `energy_kwh` | numeric | Energy attributed to the appliance for the period |
| `share` | numeric | Fraction of total meter consumption (0–1) |
| `period_start` | timestamptz | Start of the aggregation window |
| `period_end` | timestamptz | End of the aggregation window |
| `source` | text | `ml` or `submeter` |
| `created_at` | timestamptz | Row creation time |

### Endpoint

- `GET /api/meters/:meterId/appliances` — returns the per-appliance breakdown
  for the meter: each appliance's `appliance_type`, `energy_kwh`, `share` and
  the trend over recent periods, plus the total consumption for comparison.

### Saving Recommendations

For each identified appliance the backend generates energy-saving
recommendations based on its usage trend, e.g. shifting high-draw appliances
(HVAC, EV charger, water heater) to off-peak hours, adjusting thermostat
setpoints, or flagging always-on standby load. Recommendations are returned
alongside the breakdown from the appliances endpoint.

## Meter Remote Diagnostics (#993)

Technicians can troubleshoot meters over the network instead of visiting site.

### MQTT Topics

| Topic | Direction | Description |
| --- | --- | --- |
| `solargrid/meters/{meterId}/diagnostics` | backend → meter | Diagnostic command to execute |
| `solargrid/meters/{meterId}/diagnostics-result` | meter → backend | Result of the executed command |

### Supported Commands

- `reboot` — restart the meter firmware.
- `self-test` — run the meter's built-in self-test suite.
- `check-connectivity` — verify network/MQTT connectivity and report latency.

### `diagnostic_logs` Table

| Column | Type | Description |
| --- | --- | --- |
| `id` | uuid | Primary key |
| `meter_id` | uuid | Meter the diagnostic ran against |
| `command` | text | `reboot`, `self-test` or `check-connectivity` |
| `status` | text | `pending`, `success` or `failed` |
| `result` | jsonb | Raw result payload reported by the meter |
| `message` | text | Human-readable summary or error |
| `created_at` | timestamptz | When the command was issued |
| `completed_at` | timestamptz | When the result was received |

### Endpoints

- `POST /api/meters/:meterId/diagnostics` — triggers a diagnostic command;
  publishes to `solargrid/meters/{meterId}/diagnostics` and records a
  `pending` row in `diagnostic_logs`.
- `GET /api/meters/:meterId/diagnostic-logs` — returns the diagnostic history
  for the meter, newest first.

### Common Issue Patterns and Suggested Fixes

| Pattern | Suggested fix |
| --- | --- |
| No heartbeat / `check-connectivity` timeout | Check power supply and network coverage; reboot the meter |
| Repeated `self-test` failures | Schedule on-site hardware inspection |
| High error rate with successful connectivity | Inspect firmware version; reflash or update |
| Meter clock drift reported by self-test | Re-sync the meter clock via NTP |
| Frequent reboots in the log | Check for unstable power or thermal issues |
=======
