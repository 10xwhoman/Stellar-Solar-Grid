"use client";

/**
 * Meter health dashboard (#834): red/yellow/green status for every meter.
 */
import { useEffect, useState } from "react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  Legend,
  ResponsiveContainer,
} from "recharts";
import { env } from "@/lib/env";
import { getPowerQuality, type PowerQualityResponse } from "@/services/meterService";

const API = env.NEXT_PUBLIC_BACKEND_URL;

type Status = "green" | "yellow" | "red";
type MeterHealth = {
  meterId: string;
  status: Status;
  lastHeartbeat: string;
  errorRate: number;
  avgResponseTimeMs: number | null;
  uptimePercent: number;
};

const DOT: Record<Status, string> = {
  green: "bg-green-500",
  yellow: "bg-yellow-400",
  red: "bg-red-500",
};

const VOLTAGE_MIN = 207;
const VOLTAGE_MAX = 253;
const FREQUENCY_MIN = 49.5;
const FREQUENCY_MAX = 50.5;
const THD_MAX = 8;

function metricClass(ok: boolean): string {
  return ok ? "text-green-600" : "text-red-600 font-semibold";
}

function severityClass(severity: string): string {
  switch (severity.toLowerCase()) {
    case "critical":
    case "high":
      return "bg-red-100 text-red-700";
    case "medium":
    case "warning":
      return "bg-yellow-100 text-yellow-700";
    default:
      return "bg-blue-100 text-blue-700";
  }
}

function PowerQualitySection({ meterId }: { meterId: string }) {
  const [pq, setPq] = useState<PowerQualityResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = () =>
      getPowerQuality(meterId, 7)
        .then((d) => {
          if (!cancelled) setPq(d);
        })
        .catch((e) => {
          if (!cancelled) setError(e.message);
        });
    load();
    const id = setInterval(load, 60_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [meterId]);

  if (error) return <p className="text-red-600">Failed to load power quality: {error}</p>;
  if (!pq) return <p className="text-gray-500">Loading power quality…</p>;

  const current = pq.current;
  const voltageOk = current.voltage >= VOLTAGE_MIN && current.voltage <= VOLTAGE_MAX;
  const frequencyOk = current.frequency >= FREQUENCY_MIN && current.frequency <= FREQUENCY_MAX;
  const thdOk = current.thd <= THD_MAX;

  const trend = pq.history.map((h) => ({
    time: new Date(h.timestamp).toLocaleString(),
    voltage: h.voltage,
    frequency: h.frequency,
    thd: h.thd,
  }));

  return (
    <section className="mt-10">
      <h2 className="text-xl font-bold mb-4">Power Quality — {meterId}</h2>

      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-6">
        <div className="border rounded p-4">
          <div className="text-sm text-gray-500">Voltage</div>
          <div className={`text-2xl ${metricClass(voltageOk)}`}>{current.voltage.toFixed(1)} V</div>
          <div className="text-xs text-gray-400">
            Range {VOLTAGE_MIN}–{VOLTAGE_MAX} V
          </div>
        </div>
        <div className="border rounded p-4">
          <div className="text-sm text-gray-500">Frequency</div>
          <div className={`text-2xl ${metricClass(frequencyOk)}`}>{current.frequency.toFixed(2)} Hz</div>
          <div className="text-xs text-gray-400">
            Range {FREQUENCY_MIN}–{FREQUENCY_MAX} Hz
          </div>
        </div>
        <div className="border rounded p-4">
          <div className="text-sm text-gray-500">THD</div>
          <div className={`text-2xl ${metricClass(thdOk)}`}>{current.thd.toFixed(1)} %</div>
          <div className="text-xs text-gray-400">Max {THD_MAX} %</div>
        </div>
      </div>

      <h3 className="text-lg font-semibold mb-2">Detected Issues</h3>
      {pq.issues.length === 0 ? (
        <p className="text-green-600 mb-6">No power quality issues detected.</p>
      ) : (
        <ul className="mb-6 space-y-2">
          {pq.issues.map((issue, i) => (
            <li key={i} className="flex items-center gap-3 border rounded p-3">
              <span className={`px-2 py-0.5 rounded text-xs uppercase ${severityClass(issue.severity)}`}>
                {issue.severity}
              </span>
              <span className="font-medium">{issue.type}</span>
              <span className="text-sm text-gray-600">{issue.message}</span>
              <span className="ml-auto text-xs text-gray-400">
                {new Date(issue.timestamp).toLocaleString()}
              </span>
            </li>
          ))}
        </ul>
      )}

      <h3 className="text-lg font-semibold mb-2">Historical Trend (7 days)</h3>
      <div className="w-full h-72">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={trend}>
            <XAxis dataKey="time" hide />
            <YAxis yAxisId="left" />
            <YAxis yAxisId="right" orientation="right" />
            <Tooltip />
            <Legend />
            <Line yAxisId="left" type="monotone" dataKey="voltage" stroke="#2563eb" dot={false} name="Voltage (V)" />
            <Line yAxisId="right" type="monotone" dataKey="frequency" stroke="#16a34a" dot={false} name="Frequency (Hz)" />
            <Line yAxisId="right" type="monotone" dataKey="thd" stroke="#dc2626" dot={false} name="THD (%)" />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </section>
  );
}

export default function MeterHealthPage() {
  const [data, setData] = useState<{ summary: Record<Status, number>; meters: MeterHealth[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedMeter, setSelectedMeter] = useState<string | null>(null);

  useEffect(() => {
    const load = () =>
      fetch(`${API}/api/meters/health`)
        .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
        .then(setData)
        .catch((e) => setError(e.message));
    load();
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  }, []);

  return (
    <main className="p-6 max-w-5xl mx-auto">
      <h1 className="text-2xl font-bold mb-4">Meter Health</h1>
      {error && <p className="text-red-600">Failed to load: {error}</p>}
      {data && (
        <>
          <div className="flex gap-4 mb-6">
            {(["green", "yellow", "red"] as Status[]).map((s) => (
              <div key={s} className="flex items-center gap-2">
                <span className={`inline-block w-3 h-3 rounded-full ${DOT[s]}`} aria-hidden />
                <span className="capitalize">{s}</span>: {data.summary[s]}
              </div>
            ))}
          </div>
          <div className="mb-4">
            <label htmlFor="pq-meter" className="mr-2 text-sm font-medium">
              Power quality meter:
            </label>
            <select
              id="pq-meter"
              className="border rounded px-2 py-1 text-sm"
              value={selectedMeter ?? data.meters[0]?.meterId ?? ""}
              onChange={(e) => setSelectedMeter(e.target.value)}
            >
              {data.meters.map((m) => (
                <option key={m.meterId} value={m.meterId}>
                  {m.meterId}
                </option>
              ))}
            </select>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left border-b">
                  <th className="py-2">Status</th>
                  <th>Meter</th>
                  <th>Last heartbeat</th>
                  <th>Error rate</th>
                  <th>Avg response</th>
                  <th>Uptime</th>
                </tr>
              </thead>
              <tbody>
                {data.meters.map((m) => (
                  <tr key={m.meterId} className="border-b">
                    <td className="py-2">
                      <span
                        className={`inline-block w-3 h-3 rounded-full ${DOT[m.status]}`}
                        title={m.status}
                        aria-label={m.status}
                      />
                    </td>
                    <td>{m.meterId}</td>
                    <td>{new Date(m.lastHeartbeat).toLocaleString()}</td>
                    <td>{(m.errorRate * 100).toFixed(1)}%</td>
                    <td>{m.avgResponseTimeMs === null ? "—" : `${Math.round(m.avgResponseTimeMs)} ms`}</td>
                    <td>{m.uptimePercent.toFixed(1)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {data.meters.length > 0 && (
            <PowerQualitySection meterId={selectedMeter ?? data.meters[0].meterId} />
          )}
        </>
      )}
    </main>
  );
}
