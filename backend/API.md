# Backend API

This document describes the backend HTTP API surface.

## Energy Grid Simulation Tool (#909)

The simulation tool lets operators test grid scenarios, inspect grid state,
run what-if analyses, and generate impact reports for capacity planning.

### Simulation engine

`POST /api/grid/simulate`

Runs a simulation for a given scenario and returns the resulting grid state.

Request body:

```json
{
  "scenario": {
    "name": "peak-summer-demand",
    "durationHours": 24,
    "stepMinutes": 15,
    "nodes": [
      { "id": "gen-1", "type": "generator", "capacityMw": 500, "outputMw": 420 },
      { "id": "load-1", "type": "load", "demandMw": 380 }
    ],
    "links": [
      { "from": "gen-1", "to": "load-1", "capacityMw": 600 }
    ]
  }
}
```

Response body:

```json
{
  "scenarioId": "peak-summer-demand",
  "status": "ok",
  "steps": [
    {
      "t": 0,
      "nodes": [
        { "id": "gen-1", "outputMw": 420, "utilization": 0.84 },
        { "id": "load-1", "demandMw": 380, "served": true }
      ],
      "links": [
        { "from": "gen-1", "to": "load-1", "flowMw": 380, "utilization": 0.63 }
      ]
    }
  ],
  "summary": {
    "peakDemandMw": 380,
    "unservedMw": 0,
    "overloadedLinks": []
  }
}
```

### Scenario builder

`POST /api/grid/scenarios`

Creates a reusable scenario definition. The body accepts the same `scenario`
object as the simulate endpoint.

`GET /api/grid/scenarios` — list saved scenarios.

`GET /api/grid/scenarios/{id}` — fetch a single scenario.

`PUT /api/grid/scenarios/{id}` — update a scenario.

`DELETE /api/grid/scenarios/{id}` — remove a scenario.

### Visualization

`GET /api/grid/scenarios/{id}/state`

Returns the latest simulated grid state as a render-ready payload for the
frontend (nodes with positions/status and links with flow values).

```json
{
  "scenarioId": "peak-summer-demand",
  "nodes": [
    { "id": "gen-1", "type": "generator", "status": "nominal", "utilization": 0.84 },
    { "id": "load-1", "type": "load", "status": "served", "utilization": 0.63 }
  ],
  "links": [
    { "from": "gen-1", "to": "load-1", "flowMw": 380, "status": "nominal" }
  ]
}
```

### What-if analysis

`POST /api/grid/scenarios/{id}/what-if`

Applies one or more overrides to a scenario and returns the delta against the
baseline simulation.

Request body:

```json
{
  "overrides": [
    { "nodeId": "gen-1", "field": "outputMw", "value": 300 },
    { "nodeId": "load-1", "field": "demandMw", "value": 450 }
  ]
}
```

Response body:

```json
{
  "baseline": { "unservedMw": 0, "peakDemandMw": 380 },
  "modified": { "unservedMw": 70, "peakDemandMw": 450 },
  "delta": { "unservedMw": 70, "peakDemandMw": 70 }
}
```

### Report generation

`POST /api/grid/scenarios/{id}/report`

Generates an impact report for a scenario (optionally with what-if overrides)
for capacity planning.

Request body:

```json
{
  "format": "json",
  "overrides": []
}
```

Response body:

```json
{
  "scenarioId": "peak-summer-demand",
  "generatedAt": "2024-01-01T00:00:00Z",
  "impact": {
    "peakDemandMw": 380,
    "unservedMw": 0,
    "overloadedLinks": [],
    "headroomMw": 120
  },
  "recommendations": [
    "Generator gen-1 has 16% headroom at peak demand."
  ]
}
```


## API Key Management (#833)

Providers can create API keys for programmatic access. Keys are stored as
SHA-256 hashes (`api_keys` table: `id`, `provider_id`, `key_hash`,
`permissions`, `expires_at`, `revoked_at`, …); the plaintext key is returned
only once. Management routes require `X-Admin-Key` and `X-Provider-Id`.

### `POST /api/keys/generate`

Body: `{ "name"?: string, "permissions"?: ("read"|"write"|"admin")[], "expiresInDays"?: number }`

`201` → `{ "key": "sg_…", "id": "…", "provider_id": "…", "permissions": ["read"], "expires_at": null, … }`

### `GET /api/keys`

Lists the provider's keys (no secrets): `{ "keys": [ … ] }`

### `DELETE /api/keys/:keyId`

Revokes a key. `204` on success, `404` if not found.

### Authenticating with a key

Send the key in the `X-API-Key` header. Routes protected with the
`requireApiKey(permission?)` middleware respond `401` for missing, invalid,
expired or revoked keys and `403` if the key lacks the required permission
(`admin` implies all permissions).

## Usage Prediction (#835)

### `GET /api/meters/:meterId/prediction`

Estimates when the meter balance will reach zero. A linear regression is fit
to the meter's daily usage cost over the last 30 days and projected forward.
Predictions are cached and refreshed daily (or when the balance changes).
The balance is read from the contract unless `?balance=<stroops>` is given.

```json
{
  "meterId": "METER1",
  "balance": 3000,
  "estimatedDaysRemaining": 30.0,
  "confidenceInterval": { "low": 25.4, "high": 36.1, "level": 0.95 },
  "avgDailyUsage": 100,
  "trendPerDay": 0.1,
  "trainingDays": 30,
  "generatedAt": "2026-09-25T00:00:00.000Z"
}
```

`estimatedDaysRemaining` is `null` when there is no usage history or usage is
not trending toward depletion.

## Energy Device Registry (#897)

Registry of solar panels, inverters and meters with specs, certifications,
maintenance schedules and performance telemetry.

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/api/devices` | Register a device (`type`, `owner`, `manufacturer`, `model`, `serialNumber`, optional `meterId`, `location`, `installedAt`, `specs`). `409` if manufacturer + serial already registered. |
| `GET` | `/api/devices?owner=&type=&status=&meterId=&limit=&offset=` | List devices |
| `GET` | `/api/devices/:id` | Device with `certifications` and `maintenance` |
| `PATCH` | `/api/devices/:id` | Update `status`, `location`, `installedAt`, `meterId`, `specs` (merged) |
| `DELETE` | `/api/devices/:id` | Remove a device and its records |
| `POST` | `/api/devices/:id/certifications` | Add a certification (`standard`, `issuer`, `issuedAt`, optional `expiresAt`, `certificateNumber`, `documentUrl`) |
| `DELETE` | `/api/devices/:id/certifications/:certId` | Remove a certification |
| `GET` | `/api/devices/certifications/expiring?withinDays=30` | Certifications expiring soon (or expired) |
| `POST` | `/api/devices/:id/maintenance` | Schedule recurring maintenance (`task`, `intervalDays`, optional `nextDueAt`, `notes`) |
| `POST` | `/api/devices/:id/maintenance/:scheduleId/complete` | Mark done; `nextDueAt` rolls forward by `intervalDays` |
| `GET` | `/api/devices/maintenance/due?withinDays=7` | Maintenance due across all devices |
| `POST` | `/api/devices/:id/performance` | Record a reading (`powerW`, `energyKwh`, `voltageV`, `temperatureC`, `efficiency`) |
| `GET` | `/api/devices/:id/performance?days=7` | Readings plus a summary (energy, avg/peak power, capacity factor) |

Recognised `specs` keys: `ratedPowerW` (used for capacity factor), `efficiency`,
`latitude` / `longitude` (used for weather alerts), plus any free-form values.

**IoT bridge:** devices publish telemetry to `solargrid/devices/{deviceId}/telemetry`
with the same JSON body as `POST /performance` (optional `timestamp`). Messages
for unregistered device ids are dropped.

**Reminders:** an hourly worker sends a `device.maintenance_due` webhook to every
webhook registered for the device owner (provider id) when a task is due within
`MAINTENANCE_REMINDER_LEAD_DAYS` (default 3) or overdue — at most once a day per
task — and logs expiring certifications. Telemetry older than
`DEVICE_PERFORMANCE_RETENTION_DAYS` (default 90) is pruned.

## GraphQL API (#898)

`POST /graphql` (or `/api/graphql`); open `GET /graphql` in a browser for the
playground. The full reference is generated from the schema with
`npm run docs:graphql` → `docs/graphql/README.md` and `docs/graphql/schema.graphql`.

- Typed queries/mutations cover meters, payments, usage, meter health,
  predictions, solar forecast, devices, weather and staking.
- Every other REST endpoint is available through `rest(path:)` (GET, on
  `Query`) and `rest(method:, path:, body:)` (on `Mutation`); the request's
  `Authorization`, `X-Admin-Key` and `X-API-Key` headers are forwarded so REST
  auth rules still apply.
- Limits: depth ≤ `GRAPHQL_MAX_DEPTH` (8), complexity ≤ `GRAPHQL_MAX_COMPLEXITY`
  (1000). Exceeding either returns `400` with `DEPTH_LIMIT` /
  `COMPLEXITY_LIMIT`. The computed cost is returned in `extensions.cost`.
- Meter, balance and payment lookups are memoised per request, and payment
  events are fetched once per request and grouped by meter.

```graphql
query {
  meter(id: "METER1") {
    balance
    health { status uptimePercent }
    prediction { estimatedDaysRemaining }
    devices { model performance(days: 7) { totalEnergyKwh capacityFactor } }
  }
  weather(lat: 6.52, lon: 3.38) {
    productionForecast(capacityKw: 5) { date expectedKwh weatherFactor }
    alerts { date type severity message }
  }
  stats: rest(path: "/api/stats/summary") { status body }
}
```

## Energy Token Staking (#899)

Read-only stats; staking transactions are signed by the user's wallet against
the contract (`stake`, `request_unstake`, `withdraw_unstaked`,
`cancel_unstake`, `claim_staking_rewards`).

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/staking/stats` | `totalStaked`, `stakerCount`, `rewardReserve`, `totalDistributed`, `aprPercent`, `reserveRunwayDays`, `cooldownSecs` |
| `GET` | `/api/staking/:address` | `staked`, `pendingRewards`, `unstaking`, `unlockAt`, `canWithdraw`, `votingPower`, `votingSharePercent` |
| `GET` | `/api/staking/:address/voting-power` | Governance voting power (active stake) |

Amounts are strings in token base units (7 decimals). `aprPercent` assumes the
stake and reward tokens have equal value.

## Weather Integration (#900)

Backed by OpenWeatherMap One Call 3.0 (`OPENWEATHERMAP_API_KEY`). One upstream
call returns current conditions, the daily forecast and official alerts, and is
cached per location (coords rounded to ~1 km) for 30 minutes; concurrent requests
share one in-flight call, a daily call budget is enforced, and stale data (≤ 24h)
is served if the provider fails or the budget runs out.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/weather/current?lat=&lon=` | Current conditions |
| `GET` | `/api/weather/forecast?lat=&lon=` | 7-day daily forecast |
| `GET` | `/api/weather/production-forecast?lat=&lon=&capacityKw=&peakSunHours=5&efficiency=0.2&panelAgeYears=0` | Clear-sky baseline and weather-adjusted kWh per day for 7 days |
| `GET` | `/api/weather/alerts?lat=&lon=` | Production-impacting alerts: `low_production`, `storm`, `snow`, `extreme_heat`, `high_wind`, `provider` |
| `GET` | `/api/weather/correlation?deviceId=&lat=&lon=` | Pearson correlation of observed cloud cover / temperature with the device's daily energy |
| `GET` | `/api/weather/usage` | Calls made today vs. budget, cached locations |

Weather factor = cloud attenuation (Kasten–Czeplak: `1 − 0.75·c^3.4`) × thermal
derate (−0.4 %/°C of cell temperature above 25 °C, cell ≈ ambient + 20 °C);
snow caps output at 20 %.

Every 3 hours a watcher checks the forecast for each active solar panel with
`specs.latitude`/`specs.longitude` (one call per site) and sends a
`weather.production_alert` webhook to the owner's registered webhooks,
once per (site, date, alert type). Responses return `503` when no API key is
configured, `429` when the budget is exhausted with no cached data, and `502`
on provider errors.
