# Issue #886: Global Rate Limiting

Every request is charged to an IP-based fixed-window quota before JSON parsing. If a valid `X-API-Key` is present, the owning provider is also charged to a tier quota based on the key's `read`, `write`, or `admin` permission. The tighter payment and write limiters remain in force for their routes.

Configure `RATE_LIMIT_WINDOW_MS` and `RATE_LIMIT_MAX` for the shared IP quota, and `RATE_LIMIT_USER_READ_MAX`, `RATE_LIMIT_USER_WRITE_MAX`, and `RATE_LIMIT_USER_ADMIN_MAX` for provider quotas. Configure `REDIS_URL` to share atomic counters across replicas; without Redis the backend falls back to a bounded process-local store. `TRUST_PROXY_HOPS` must match the trusted proxy chain or clients may be misidentified. Responses include `X-RateLimit-*`, `RateLimit-*`, and `Retry-After` headers; CORS exposes those headers to browser clients.

# Issue #882: Email Notifications

`POST /api/email-notifications/price-alert` sends an admin-triggered price alert. Billing notifications are emitted for successful meter payments and generated bills, and low-balance email notifications use the meter's billing account address. `GET /api/email-notifications/manage?token=...` offers per-event controls; signed tokens are linked in delivered messages. `GET /api/email-notifications/unsubscribe?token=...` displays a confirmation page and the subsequent form POST unsubscribes from all email categories.

Set `EMAIL_PROVIDER` to `resend`, `sendgrid`, or `log`, and configure the corresponding provider API key, `EMAIL_FROM`, `EMAIL_PREFERENCES_SECRET`, and `EMAIL_PREFERENCES_BASE_URL`. The log provider does not deliver mail. Delivery rates are provider/deployment dependent and must be measured from provider events; the application cannot guarantee a 95% deliverability rate by itself.

# Issue #885: Grid Stability

Device voltage and frequency readings are accepted over `solargrid/devices/{deviceId}/telemetry`, HTTP `POST /api/devices/:id/performance`, and GraphQL `recordDevicePerformance`. `GET /api/devices/:id/stability?days=7` returns the measured-readings stability score and historical anomalies. Anomalies are persisted and sent to device-owner webhooks with a per-device/metric 15-minute alert cooldown. Set `GRID_VOLTAGE_MIN_V`, `GRID_VOLTAGE_MAX_V`, `GRID_FREQUENCY_MIN_HZ`, and `GRID_FREQUENCY_MAX_HZ` to match the local grid standard.
# Backend API

This document describes the backend HTTP API surface.

## Energy Forecasting Service (#881)

`GET /api/weather/energy-forecast`

Returns hourly production and/or consumption estimates for up to 48 hours. At
least one of `meterId` or `deviceId` is required. A `deviceId` must identify a
registered `solar_panel`; its `ratedPowerW`, `latitude`, and `longitude` specs
are used when available. `lat` and `lon` may be supplied explicitly and are
required for meter-only requests. `hours` defaults to 48 and may be an integer
from 1 through 48.

Example:

```text
GET /api/weather/energy-forecast?meterId=METER_01&deviceId=PANEL_01&hours=48
```

Consumption training reads the previous 90 days of meter usage and converts
the platform's milli-kWh event units to kWh. Production training uses the
previous 90 days of registered solar-panel performance telemetry. Both models
use a regularized seasonal regression over hour-of-day and weekday features;
missing hours are treated as zero. OpenWeather hourly cloud cover and
temperature adjust production estimates; temperature adjusts consumption
estimates. Precipitation probability is included as forecast context. Models retrain at
startup and every six hours by default; configure the interval with
`ENERGY_FORECAST_RETRAIN_INTERVAL_MS`.

The response includes per-stream `trainingSamples`, `accuracyPct`, and
`trainedAt`. Accuracy is a held-out weighted absolute-error score and is `null`
when there is no usable validation history; the service does not claim a fixed
accuracy for meters or sites without representative historical data.

```json
{
  "horizonHours": 48,
  "weatherStale": false,
  "models": {
    "production": { "algorithm": "seasonal-ridge", "trainingSamples": 1200, "accuracyPct": 91.2 },
    "consumption": { "algorithm": "seasonal-ridge", "trainingSamples": 2160, "accuracyPct": 88.5 }
  },
  "forecast": [
    {
      "timestamp": "2026-09-29T12:00:00.000Z",
      "productionKwh": 2.15,
      "consumptionKwh": 0.42,
      "weather": { "temperatureC": 22, "cloudCoverPct": 18, "productionFactor": 0.91 }
    }
  ]
}
```

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

## Widget Summary (#901)

### `GET /api/widgets/summary?meterId=<id>`

A compact payload (under 1 KB) for the iOS and Android home-screen widgets. It is cached for 5 minutes and served with
an `ETag`, so send `If-None-Match` to get a `304` when nothing has changed. See `docs/MOBILE_WIDGETS.md`.

```json
{
  "meterId": "METER1",
  "active": true,
  "balanceXlm": 12.5,
  "todayUnits": 3.2,
  "last7DaysUnits": [4.1, 3.9, 5.0, 4.4, 3.8, 4.0, 3.2],
  "daysRemaining": 3.1,
  "updatedAt": "2026-09-27T10:00:00.000Z"
}
```

### Live energy flow (#870)

- `GET /api/widgets/energy?meterId=<id>&range=5m|hourly|daily` returns up to 12 five-minute,
  24 hourly, or 7 daily production/consumption buckets.
- `WS /api/widgets/live?meterId=<id>&range=5m|hourly|daily` sends an initial snapshot and
  refreshes every five seconds. The frontend reconnects with capped exponential backoff and
  polls the HTTP route while disconnected.
- Consumption comes from meter usage events. Production comes from active linked solar-panel
  device telemetry (`energyKwh`, or power integrated over the bucket).

## Energy Trading Simulator (#929)

Practice-only cash and energy balances are stored separately from on-chain accounts. The
simulator verifies that the supplied Stellar wallet owns its meter before account reads/trades.
The market feed defaults to `energy-charts.info` for bidding zone `DE-LU`; set
`ENERGY_MARKET_DATA_URL`, `ENERGY_MARKET_BIDDING_ZONE`, and
`MARKET_PRICE_FALLBACK_EUR_KWH` to configure the feed and offline quote.

- `GET /api/simulator/market?hours=24` returns historical/current EUR-per-kWh quotes and the
  configured practice fee rate.
- `GET /api/simulator/account?meterId=<id>&stellarAddress=<G...>` returns the virtual account
  and recent practice trades.
- `POST /api/simulator/trade` accepts `{ meterId, stellarAddress, side, quantityKwh }`.
- `GET /api/simulator/leaderboard?limit=20` returns anonymized portfolio rankings.

Accounts start with `SIMULATOR_STARTING_CREDITS` (default 1,000); the default virtual trade fee
is 0.5%. Simulator transactions never call the Stellar contract.

## Energy Theft Detection (#931)

The backend scans newly recorded usage at `THEFT_SCAN_INTERVAL_MS` (default one minute) and
compares readings with a robust per-meter median/MAD baseline. Alerts are persisted once per
usage event and sent to registered provider webhooks immediately after detection. The default
threshold is intentionally conservative; the monthly report tracks investigated false positives
so providers can validate the under-5% target against their own metering population.

All theft routes require the existing admin session or `X-Admin-Key` credential:

- `GET /api/theft/alerts?status=open&limit=50&offset=0`
- `GET /api/theft/alerts/:id/investigation`
- `PATCH /api/theft/alerts/:id/investigation` with `{ status, assignedTo?, note?, actor? }`
- `GET /api/theft/reports/monthly?month=YYYY-MM` generates and stores the selected monthly report.

## Monthly Bills (#902)

Bills are generated on the 1st of each month, emailed as PDFs with a payment link, and kept as a permanent history.
Endpoints live under `/api/billing`. See `docs/BILLING.md`.

## Competitions (#903)

Monthly efficiency, trading and green-energy competitions with live SSE leaderboards and automatic prize payouts.
Endpoints live under `/api/competitions`. See `docs/COMPETITIONS.md`.

## Smart Home (#904)

- Failed webhook calls are logged but do not crash the IoT bridge
- Webhook timeouts can be configured via your HTTP client settings
- Consider idempotency keys on your webhook endpoint to handle retries

## Load Balancing (#889)
`POST /api/load-balancing/balance` — body `{ capacityKw, pricePerKwh, peakPriceThreshold?, loads: [{ id, demandKw, priority: "critical"|"high"|"normal"|"deferrable", override?: "on"|"off" }] }`.
Returns `{ on, off, servedKw, shedKw, baselineCost, optimisedCost, savingsPct }`. Critical loads are always served; `override` lets users force a load on/off; deferrable loads are shifted when the price exceeds `peakPriceThreshold`.

## Two-Factor Authentication (#890)
- `POST /api/2fa/enroll` `{ account, phone? }` → TOTP secret, `otpauthUrl` for authenticator apps, 10 single-use recovery codes.
- `POST /api/2fa/verify` `{ account, code, method?: "totp"|"sms"|"recovery" }` — 5 failures lock the account for 15 min.
- `POST /api/2fa/sms` `{ account }` — sends SMS fallback code (5-min expiry).
- `POST /api/2fa/recovery-codes` `{ account, code }` — regenerates recovery codes.
- Enforcement: `requireTwoFactor` middleware requires 2FA for accounts with value ≥ `TWO_FACTOR_ENFORCE_THRESHOLD`.

## Trading Bot API (#891)
See `docs/TRADING_API.md`.
Google Home and Alexa account linking (OAuth 2.0), device fulfillment, energy routines and privacy controls. Endpoints
live under `/api/smart-home`. See `docs/SMART_HOME.md`.

## Technician Portal (#991)

Field technicians use these routes to manage installations and maintenance work
orders. The mobile-optimized portal lives at `/technician`. See
`docs/TECHNICIAN_PORTAL.md`.

### `POST /api/work-orders`

Creates a work order and optionally assigns it to a technician.

Body:

```json
{
  "type": "installation",
  "meterId": "METER1",
  "technicianId": "TECH1",
  "priority": "high",
  "scheduledFor": "2026-10-01T09:00:00.000Z",
  "notes": "Replace meter and verify seal"
}
```

`201` → the created work order, including its `id` and `status` (`pending`,
`assigned`, `in_progress`, `completed`, or `cancelled`).

### `GET /api/work-orders/assigned/:techId`

Lists the work orders assigned to a technician, newest first. Optional query
parameters: `status` (filter by status) and `limit` (default 50).

```json
{
  "technicianId": "TECH1",
  "workOrders": [
    {
      "id": "WO-1001",
      "type": "installation",
      "meterId": "METER1",
      "status": "assigned",
      "priority": "high",
      "scheduledFor": "2026-10-01T09:00:00.000Z",
      "photos": []
    }
  ]
}
```

### `PATCH /api/work-orders/:id/complete`

Marks a work order complete. Body may include `completedAt`, `notes`, and
`signature` (base64-encoded customer signature). `404` if the work order does
not exist.

### `POST /api/work-orders/:id/photos`

Uploads an installation-verification photo. Send `multipart/form-data` with a
`photo` file field and an optional `caption`. `201` → `{ "id", "url",
"caption", "uploadedAt" }`. Photos are also returned by the assigned-work-orders
listing.

### QR code scanning

The portal registers meters by scanning the meter's QR code client-side and
submitting the decoded `meterId` to `POST /api/work-orders`. No separate
server-side scan endpoint is required.

## Customer Satisfaction Surveys (#994)

Surveys are delivered by email after payments or support interactions, and
responses feed the NPS analytics dashboard. See `API.md` survey sections below
and the analytics route for metric definitions.

### `POST /api/surveys`

Creates a survey definition and queues delivery to the targeted customers.

Body:

```json
{
  "name": "Post-payment satisfaction",
  "trigger": "payment",
  "questions": [
    { "id": "nps", "type": "nps", "text": "How likely are you to recommend us?" },
    { "id": "csat", "type": "rating", "text": "How satisfied are you with the service?" }
  ]
}
```

`201` → the created survey with its `id` and `status`.

### `POST /api/surveys/:id/responses`

Records a customer response. Body:

```json
{
  "customerId": "CUST1",
  "answers": [
    { "questionId": "nps", "value": 9 },
    { "questionId": "csat", "value": 5 }
  ]
}
```

`201` → the stored response. `404` if the survey does not exist; `400` if a
required answer is missing or out of range.

### `GET /api/surveys/analytics`

Returns satisfaction metrics and the Net Promoter Score for a survey or the
whole program. Optional query parameters: `surveyId` and `from`/`to` (ISO
dates).

NPS is calculated from the `nps` question on a 0–10 scale:

- **Promoters**: responses scoring 9–10.
- **Passives**: responses scoring 7–8.
- **Detractors**: responses scoring 0–6.
- **NPS** = `%promoters − %detractors`, ranging from −100 to +100.

```json
{
  "responses": 120,
  "promoters": 72,
  "passives": 30,
  "detractors": 18,
  "nps": 45.0,
  "csat": 4.3,
  "responseRate": 0.24
}
```

`csat` is the mean rating of `rating` questions normalized to a 1–5 scale and
is `null` when no rating answers exist.

## Seasonal Pricing (#992)

Seasonal rates adjust the base energy price by a multiplier for a date range.
The smart contract stores the `SeasonalRate` struct (`season`, `start_date`,
`end_date`, `multiplier`) and applies the active rate automatically based on
the current date, emitting a `SeasonalRateApplied` event. These backend routes
expose the schedule to the frontend.

### `GET /api/pricing/season`

Returns the currently active season and the effective rate.

```json
{
  "season": "summer",
  "multiplier": 1.25,
  "baseRate": 0.12,
  "effectiveRate": 0.15,
  "startDate": "2026-06-01",
  "endDate": "2026-08-31"
}
```

`season` is `null` when no seasonal rate is active.

### `PUT /api/pricing/season`

Admin-only. Creates or replaces the seasonal rate schedule. Body accepts an
array of `SeasonalRate` objects:

```json
{
  "rates": [
    { "season": "summer", "startDate": "2026-06-01", "endDate": "2026-08-31", "multiplier": 1.25 },
    { "season": "winter", "startDate": "2026-12-01", "endDate": "2027-02-28", "multiplier": 1.15 }
  ]
}
```

`200` → the stored schedule. `400` if ranges overlap or a multiplier is not
positive.

### `GET /api/pricing/season/history`

Returns historical seasonal rates and usage for comparison. Optional query
parameters: `from`/`to` (ISO dates) and `season`.

```json
{
  "history": [
    { "season": "summer", "year": 2025, "multiplier": 1.2, "avgRate": 0.144, "usageKwh": 4120 }
  ]
}
```
