import * as StellarSdk from '@stellar/stellar-sdk';
import { StellarService } from '../lib/stellar.js';
import { getMqttClient } from './mqttClient.js';
import { logger } from '../lib/logger.js';

const THRESHOLD = 0.8;
const warnedToday = new Set<string>();

// Network congestion management (#906)
const CONGESTION_THRESHOLD = Number(process.env.CONGESTION_THRESHOLD ?? 0.85);
const CONGESTION_CLEAR_THRESHOLD = Number(process.env.CONGESTION_CLEAR_THRESHOLD ?? 0.7);
const CONGESTION_PEAK_MULTIPLIER = Number(process.env.CONGESTION_PEAK_MULTIPLIER ?? 1.5);
const CONGESTION_SHED_RATIO = Number(process.env.CONGESTION_SHED_RATIO ?? 0.5);

let congested = false;
const congestionHistory: Array<{ at: string; load: number; congested: boolean }> = [];

// Limit watcher initialization guard to prevent duplicate startup
let watcherStarted = false;

function scheduleWarnedReset() {
  const now = new Date();
  const midnight = new Date(now);
  midnight.setHours(24, 0, 0, 0);
  setTimeout(() => {
    warnedToday.clear();
    scheduleWarnedReset();
  }, midnight.getTime() - now.getTime());
}

/**
 * Detect network congestion from aggregate load signals and, when the state
 * changes, apply dynamic pricing, shed non-critical loads, and notify users.
 */
export async function checkNetworkCongestion(stellar: StellarService) {
  try {
    const raw = await stellar.query('get_all_meters', []);
    const meters = (StellarSdk.scValToNative(raw) as any[]) ?? [];
    if (meters.length === 0) return;

    let totalLoad = 0;
    let totalCapacity = 0;
    for (const meter of meters) {
      totalLoad += Number(meter.current_load ?? 0);
      totalCapacity += Number(meter.capacity ?? meter.daily_limit ?? 0);
    }
    if (totalCapacity <= 0) return;

    const load = totalLoad / totalCapacity;
    const wasCongested = congested;
    if (!congested && load >= CONGESTION_THRESHOLD) congested = true;
    else if (congested && load <= CONGESTION_CLEAR_THRESHOLD) congested = false;

    congestionHistory.push({ at: new Date().toISOString(), load, congested });
    if (congestionHistory.length > 288) congestionHistory.shift();

    if (congested === wasCongested) return;

    const mqtt = getMqttClient();
    if (congested) {
      // Dynamic pricing during congestion.
      const priceMultiplier = CONGESTION_PEAK_MULTIPLIER;
      // Load shedding: shed non-critical loads only, keep critical loads online.
      const shedMeters = meters
        .filter((m) => !m.critical && Number(m.current_load ?? 0) > 0)
        .map((m) => ({ meterId: m.id, shedTo: Number(m.current_load) * (1 - CONGESTION_SHED_RATIO) }));

      mqtt.publish(
        'network/congestion',
        JSON.stringify({ type: 'CONGESTION_START', load, priceMultiplier, shedMeters }),
        { qos: 1 },
      );
      for (const shed of shedMeters) {
        mqtt.publish(
          `meters/${shed.meterId}/commands`,
          JSON.stringify({ type: 'SHED_LOAD', targetLoad: shed.shedTo, reason: 'CONGESTION' }),
          { qos: 1 },
        );
      }
      for (const meter of meters) {
        mqtt.publish(
          `meters/${meter.id}/notifications`,
          JSON.stringify({ type: 'CONGESTION_NOTICE', load, priceMultiplier }),
          { qos: 1 },
        );
      }
      logger.info('Network congestion detected', { load, priceMultiplier, shedCount: shedMeters.length });
    } else {
      mqtt.publish(
        'network/congestion',
        JSON.stringify({ type: 'CONGESTION_END', load }),
        { qos: 1 },
      );
      for (const meter of meters) {
        mqtt.publish(
          `meters/${meter.id}/notifications`,
          JSON.stringify({ type: 'CONGESTION_CLEARED', load }),
          { qos: 1 },
        );
      }
      logger.info('Network congestion cleared', { load });
    }
  } catch (err) {
    logger.error('checkNetworkCongestion error', { err });
  }
}

/** Historical congestion analytics. */
export function getCongestionHistory() {
  return congestionHistory.slice();
}

export async function checkDailyLimits(stellar: StellarService) {
  try {
    const raw = await stellar.query('get_all_meters', []);
    const meters = (StellarSdk.scValToNative(raw) as any[]) ?? [];
    for (const meter of meters) {
      if (Number(meter.daily_limit) > 0) {
        const ratio = Number(meter.day_spent) / Number(meter.daily_limit);
        if (ratio >= THRESHOLD && !warnedToday.has(meter.id)) {
          getMqttClient().publish(
            `meters/${meter.id}/warnings`,
            JSON.stringify({ type: 'DAILY_LIMIT_WARNING', ratio, meterId: meter.id }),
            { qos: 1 },
          );
          warnedToday.add(meter.id);
          logger.info('Daily limit warning published', { meterId: meter.id, ratio });
        }
      }
    }
  } catch (err) {
    logger.error('checkDailyLimits error', { err });
  }
}

export function startLimitWatcher(stellar: StellarService) {
  if (watcherStarted) {
    logger.warn("Limit watcher already started, skipping duplicate initialization");
    return;
  }
  watcherStarted = true;
  scheduleWarnedReset();
  const intervalMs = Number(process.env.LIMIT_WATCH_INTERVAL_MS ?? 5 * 60 * 1000);
  setInterval(() => checkDailyLimits(stellar), intervalMs);
  setInterval(() => checkNetworkCongestion(stellar), intervalMs);
}
