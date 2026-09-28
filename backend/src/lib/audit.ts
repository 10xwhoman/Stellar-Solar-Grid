import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import Database from "better-sqlite3";

const DB_PATH =
  process.env.AUDIT_DB_PATH ??
  path.resolve(process.cwd(), "data", "audit.sqlite");

export type AuditEntry = {
  id: number;
  timestamp: string;
  action: string;
  actor: string | null;
  meter_id: string | null;
  amount: string | null;
  tx_hash: string | null;
  details: string | null;
  prev_hash: string;
  hash: string;
};

export type AuditInput = {
  action: string;
  actor?: string | null;
  meterId?: string | null;
  amount?: string | number | null;
  txHash?: string | null;
  details?: unknown;
};

export type AuditFilter = {
  action?: string;
  actor?: string;
  meterId?: string;
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
};

const GENESIS = "0".repeat(64);

const db = (() => {
  fs.mkdirSync(path.dirname(DB_PATH), { recursive: true });
  const d = new Database(DB_PATH);
  d.pragma("journal_mode = WAL");
  d.exec(`
    CREATE TABLE IF NOT EXISTS audit_log (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      timestamp TEXT NOT NULL,
      action TEXT NOT NULL,
      actor TEXT,
      meter_id TEXT,
      amount TEXT,
      tx_hash TEXT,
      details TEXT,
      prev_hash TEXT NOT NULL,
      hash TEXT NOT NULL UNIQUE
    );
    CREATE INDEX IF NOT EXISTS idx_audit_meter ON audit_log (meter_id, timestamp);
    CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log (action, timestamp);
    -- Append-only: block updates and deletes at the storage layer.
    CREATE TRIGGER IF NOT EXISTS audit_no_update BEFORE UPDATE ON audit_log
      BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
    CREATE TRIGGER IF NOT EXISTS audit_no_delete BEFORE DELETE ON audit_log
      BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
  `);
  return d;
})();

function computeHash(e: Omit<AuditEntry, "id" | "hash">): string {
  const payload = [
    e.prev_hash, e.timestamp, e.action, e.actor ?? "", e.meter_id ?? "",
    e.amount ?? "", e.tx_hash ?? "", e.details ?? "",
  ].join("|");
  return crypto.createHash("sha256").update(payload).digest("hex");
}

/** Append a hash-chained entry. Each hash commits to the previous one. */
export const recordAudit = db.transaction((input: AuditInput): AuditEntry => {
  const last = db
    .prepare("SELECT hash FROM audit_log ORDER BY id DESC LIMIT 1")
    .get() as { hash: string } | undefined;
  const base = {
    timestamp: new Date().toISOString(),
    action: input.action,
    actor: input.actor ?? null,
    meter_id: input.meterId ?? null,
    amount: input.amount == null ? null : String(input.amount),
    tx_hash: input.txHash ?? null,
    details: input.details == null ? null : JSON.stringify(input.details),
    prev_hash: last?.hash ?? GENESIS,
  };
  const hash = computeHash(base);
  const { lastInsertRowid } = db
    .prepare(
      `INSERT INTO audit_log (timestamp, action, actor, meter_id, amount, tx_hash, details, prev_hash, hash)
       VALUES (@timestamp, @action, @actor, @meter_id, @amount, @tx_hash, @details, @prev_hash, @hash)`,
    )
    .run({ ...base, hash });
  return { id: Number(lastInsertRowid), ...base, hash };
});

export function queryAudit(f: AuditFilter = {}): AuditEntry[] {
  const where: string[] = [];
  const params: Record<string, unknown> = {};
  if (f.action) { where.push("action = @action"); params.action = f.action; }
  if (f.actor) { where.push("actor = @actor"); params.actor = f.actor; }
  if (f.meterId) { where.push("meter_id = @meterId"); params.meterId = f.meterId; }
  if (f.from) { where.push("timestamp >= @from"); params.from = f.from; }
  if (f.to) { where.push("timestamp <= @to"); params.to = f.to; }
  params.limit = Math.min(Math.max(f.limit ?? 100, 1), 10_000);
  params.offset = Math.max(f.offset ?? 0, 0);
  const sql = `SELECT * FROM audit_log ${where.length ? "WHERE " + where.join(" AND ") : ""}
    ORDER BY id ASC LIMIT @limit OFFSET @offset`;
  return db.prepare(sql).all(params) as AuditEntry[];
}

/** Walk the full chain and report the first broken link, if any. */
export function verifyAuditChain(): { valid: boolean; checked: number; brokenAt?: number } {
  let prev = GENESIS;
  let checked = 0;
  for (const row of db.prepare("SELECT * FROM audit_log ORDER BY id ASC").iterate() as Iterable<AuditEntry>) {
    const { id, hash, ...rest } = row;
    if (row.prev_hash !== prev || computeHash(rest) !== hash) {
      return { valid: false, checked, brokenAt: id };
    }
    prev = hash;
    checked++;
  }
  return { valid: true, checked };
}

export function complianceReport(from?: string, to?: string) {
  const params = { from: from ?? "0000", to: to ?? "9999" };
  const byAction = db
    .prepare(
      `SELECT action, COUNT(*) AS count, SUM(CAST(amount AS REAL)) AS total_amount,
              SUM(tx_hash IS NOT NULL) AS on_chain
       FROM audit_log WHERE timestamp BETWEEN @from AND @to GROUP BY action`,
    )
    .all(params);
  return {
    period: { from: from ?? null, to: to ?? null },
    generatedAt: new Date().toISOString(),
    integrity: verifyAuditChain(),
    byAction,
  };
}

const CSV_COLUMNS: (keyof AuditEntry)[] = [
  "id", "timestamp", "action", "actor", "meter_id", "amount", "tx_hash", "details", "prev_hash", "hash",
];

export function toCsv(rows: AuditEntry[]): string {
  const esc = (v: unknown) => {
    const s = v == null ? "" : String(v);
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return [CSV_COLUMNS.join(","), ...rows.map((r) => CSV_COLUMNS.map((c) => esc(r[c])).join(","))].join("\n");
}

/** Minimal dependency-free PDF (Helvetica, one line per entry, paginated). */
export function toPdf(rows: AuditEntry[], title = "Energy Audit Trail"): Buffer {
  const pdfEsc = (s: string) => s.replace(/[\\()]/g, "\\$&").replace(/[^\x20-\x7e]/g, "?");
  const lines = [
    title,
    `Generated ${new Date().toISOString()} - ${rows.length} entries`,
    "",
    ...rows.map((r) =>
      `#${r.id} ${r.timestamp} ${r.action} meter=${r.meter_id ?? "-"} amt=${r.amount ?? "-"} tx=${(r.tx_hash ?? "-").slice(0, 16)} hash=${r.hash.slice(0, 16)}`,
    ),
  ];
  const perPage = 60;
  const pages: string[][] = [];
  for (let i = 0; i < lines.length; i += perPage) pages.push(lines.slice(i, i + perPage));
  if (!pages.length) pages.push([title]);

  const objs: string[] = [];
  objs[1] = "<< /Type /Catalog /Pages 2 0 R >>";
  objs[3] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";
  const kids: string[] = [];
  pages.forEach((pageLines, i) => {
    const pageId = 4 + i * 2;
    const contentId = pageId + 1;
    kids.push(`${pageId} 0 R`);
    const text = pageLines.map((l) => `(${pdfEsc(l)}) Tj T*`).join("\n");
    const stream = `BT /F1 8 Tf 10 TL 30 810 Td\n${text}\nET`;
    objs[pageId] = `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R >> >> /Contents ${contentId} 0 R >>`;
    objs[contentId] = `<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}\nendstream`;
  });
  objs[2] = `<< /Type /Pages /Kids [${kids.join(" ")}] /Count ${pages.length} >>`;

  let out = "%PDF-1.4\n";
  const offsets: number[] = [];
  for (let i = 1; i < objs.length; i++) {
    offsets[i] = Buffer.byteLength(out);
    out += `${i} 0 obj\n${objs[i]}\nendobj\n`;
  }
  const xref = Buffer.byteLength(out);
  out += `xref\n0 ${objs.length}\n0000000000 65535 f \n`;
  for (let i = 1; i < objs.length; i++) out += `${String(offsets[i]).padStart(10, "0")} 00000 n \n`;
  out += `trailer\n<< /Size ${objs.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
  return Buffer.from(out, "latin1");
/**
 * Structured audit log for admin-invoked write operations.
 *
 * Every action that passes through requireAdminKey emits a structured entry
 * to a dedicated audit log stream (separate from the general request log).
 * This gives a queryable, exportable trail of who (IP) invoked which
 * privileged action and when.
 *
 * Format: one JSON object per line — compatible with log-aggregation tooling
 * (Loki, CloudWatch Logs Insights, jq, etc.).
 *
 * Storage:
 *   - In production: writes to AUDIT_LOG_PATH (default: logs/audit.jsonl)
 *     in addition to emitting via the structured logger.
 *   - In all environments: the entry is also emitted at INFO level through
 *     the standard winston logger (tagged with `audit: true`) so it appears
 *     in any log shipper already pointed at stdout.
 *
 * Export:
 *   - GET /api/admin/audit-logs              — paginated JSON query (closes #744)
 *   - GET /api/admin/audit-logs/export?format=csv|json — bulk download (closes #744)
 */

import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
import { logger } from "./logger.js";

export type AuditEntry = {
  /** ISO-8601 timestamp */
  timestamp: string;
  /** HTTP method, e.g. "POST" */
  method: string;
  /** Route path, e.g. "/api/allowlist" */
  path: string;
  /** Request body (params, address, etc.) — secrets are never included */
  params: unknown;
  /** Requester IP — x-forwarded-for first, then socket.remoteAddress */
  ip: string;
  /** Optional: admin key identity hint (last 4 chars) for multi-key setups */
  keyHint: string;
};

// ── File sink (optional) ──────────────────────────────────────────────────────

const AUDIT_LOG_PATH =
  process.env.AUDIT_LOG_PATH ??
  path.resolve(process.cwd(), "logs", "audit.jsonl");

let auditStream: fs.WriteStream | null = null;

function getAuditStream(): fs.WriteStream | null {
  if (auditStream) return auditStream;
  if (process.env.AUDIT_LOG_DISABLE === "true") return null;

  try {
    fs.mkdirSync(path.dirname(AUDIT_LOG_PATH), { recursive: true });
    auditStream = fs.createWriteStream(AUDIT_LOG_PATH, { flags: "a" });
    auditStream.on("error", (err) => {
      logger.error("Audit log write error", { err: err.message });
    });
    return auditStream;
  } catch (err: any) {
    logger.error("Failed to open audit log file", {
      path: AUDIT_LOG_PATH,
      err: err.message,
    });
    return null;
  }
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Emit a structured audit entry for an admin-gated action.
 *
 * Call this inside requireAdminKey (or any admin-only handler) after
 * authentication has been confirmed.
 */
export function auditLog(entry: AuditEntry): void {
  const record = { audit: true, ...entry };

  // 1. Emit via the standard logger (stdout / log shipper)
  logger.info(record, "admin action audited");

  // 2. Append to the dedicated audit log file (if configured)
  const stream = getAuditStream();
  if (stream) {
    stream.write(JSON.stringify(record) + "\n");
  }
}

/**
 * Build an AuditEntry from an Express request object.
 * Must only be called after authentication has passed (key is valid).
 */
export function buildAuditEntry(
  req: {
    method: string;
    path: string;
    body?: unknown;
    headers: Record<string, string | string[] | undefined>;
    socket?: { remoteAddress?: string };
    ip?: string;
  },
): AuditEntry {
  // Prefer x-forwarded-for (set by reverse proxies) over socket IP
  const forwardedFor = req.headers["x-forwarded-for"];
  const ip =
    (Array.isArray(forwardedFor) ? forwardedFor[0] : forwardedFor)?.split(",")[0]?.trim() ??
    req.ip ??
    req.socket?.remoteAddress ??
    "unknown";

  // Produce a short identity hint from the last 4 chars of the provided key
  const rawKey = req.headers["x-admin-key"];
  const keyStr = Array.isArray(rawKey) ? rawKey[0] : rawKey ?? "";
  const keyHint = keyStr.length >= 4 ? `...${keyStr.slice(-4)}` : "****";

  return {
    timestamp: new Date().toISOString(),
    method: req.method,
    path: req.path,
    params: req.body ?? null,
    ip,
    keyHint,
  };
}

// ── Query / export helpers (closes #744) ─────────────────────────────────────

export interface AuditQueryOptions {
  /** Inclusive lower bound (ISO-8601). */
  start?: string;
  /** Inclusive upper bound (ISO-8601). */
  end?: string;
  /** Filter by event path substring (e.g. "/api/meters"). */
  eventType?: string;
  /** Max entries to return. Defaults to 500. */
  limit?: number;
  /** Number of entries to skip (for pagination). Defaults to 0. */
  offset?: number;
}

/**
 * Read the JSONL audit log file line-by-line and return matching entries.
 *
 * Parsing is streaming (readline) so large log files do not load entirely
 * into memory. The function resolves once the end of file is reached.
 *
 * Closes #744.
 */
export async function queryAuditLog(
  opts: AuditQueryOptions = {},
): Promise<{ entries: (AuditEntry & { audit?: true })[]; total: number }> {
  const { start, end, eventType, limit = 500, offset = 0 } = opts;
  const startMs = start ? new Date(start).getTime() : -Infinity;
  const endMs = end ? new Date(end).getTime() : Infinity;

  if (!fs.existsSync(AUDIT_LOG_PATH)) {
    return { entries: [], total: 0 };
  }

  const matching: (AuditEntry & { audit?: true })[] = [];

  await new Promise<void>((resolve, reject) => {
    const fileStream = fs.createReadStream(AUDIT_LOG_PATH, { encoding: "utf8" });
    const rl = readline.createInterface({ input: fileStream, crlfDelay: Infinity });

    fileStream.on("error", reject);
    rl.on("error", reject);
    rl.on("close", resolve);

    rl.on("line", (line) => {
      const trimmed = line.trim();
      if (!trimmed) return;
      try {
        const record = JSON.parse(trimmed) as AuditEntry & { audit?: true };
        if (!record.timestamp) return;

        const ts = new Date(record.timestamp).getTime();
        if (ts < startMs || ts > endMs) return;
        if (eventType && !record.path.includes(eventType)) return;

        matching.push(record);
      } catch {
        // Skip malformed lines
      }
    });
  });

  const total = matching.length;
  const entries = matching.slice(offset, offset + limit);
  return { entries, total };
}

/**
 * Serialise audit entries as a CSV string.
 * Columns: timestamp, method, path, ip, keyHint, params
 *
 * Closes #744.
 */
export function auditEntriesToCsv(
  entries: (AuditEntry & { audit?: true })[],
): string {
  const header = "timestamp,method,path,ip,keyHint,params";
  const rows = entries.map((e) => {
    const params = JSON.stringify(e.params ?? null).replace(/"/g, '""');
    return `${e.timestamp},${e.method},${e.path},${e.ip},${e.keyHint},"${params}"`;
  });
  return [header, ...rows].join("\n");
}

// ── Energy data export (closes #905) ─────────────────────────────────────────

/** Supported export serialisation formats. */
export type ExportFormat = "csv" | "json" | "xml";

/** A single energy transaction record included in a personal data export. */
export interface EnergyTransaction {
  /** ISO-8601 timestamp of the transaction. */
  timestamp: string;
  /** Meter identifier the transaction belongs to. */
  meterId: string;
  /** Transaction type, e.g. "production" | "consumption" | "trade". */
  type: string;
  /** Energy amount in kWh. */
  amountKwh: number;
  /** Optional monetary value associated with the transaction. */
  value?: number;
  /** Optional free-form metadata. */
  metadata?: Record<string, unknown>;
}

/** Options controlling which transactions are included in an export. */
export interface EnergyExportOptions {
  /** Inclusive lower bound (ISO-8601). */
  start?: string;
  /** Inclusive upper bound (ISO-8601). */
  end?: string;
  /** Filter by meter identifier. */
  meterId?: string;
  /** Filter by transaction type. */
  type?: string;
}

/**
 * Filter a user's energy transactions by date range and optional
 * meter/type filters. Used by the personal data export endpoint (#905).
 */
export function filterEnergyTransactions(
  transactions: EnergyTransaction[],
  opts: EnergyExportOptions = {},
): EnergyTransaction[] {
  const { start, end, meterId, type } = opts;
  const startMs = start ? new Date(start).getTime() : -Infinity;
  const endMs = end ? new Date(end).getTime() : Infinity;

  return transactions.filter((tx) => {
    const ts = new Date(tx.timestamp).getTime();
    if (Number.isNaN(ts) || ts < startMs || ts > endMs) return false;
    if (meterId && tx.meterId !== meterId) return false;
    if (type && tx.type !== type) return false;
    return true;
  });
}

/** Escape a value for safe inclusion in an XML text node. */
function escapeXml(value: unknown): string {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/**
 * Serialise energy transactions as CSV.
 * Columns: timestamp, meterId, type, amountKwh, value, metadata
 */
export function energyTransactionsToCsv(transactions: EnergyTransaction[]): string {
  const header = "timestamp,meterId,type,amountKwh,value,metadata";
  const rows = transactions.map((tx) => {
    const metadata = JSON.stringify(tx.metadata ?? null).replace(/"/g, '""');
    return `${tx.timestamp},${tx.meterId},${tx.type},${tx.amountKwh},${tx.value ?? ""},"${metadata}"`;
  });
  return [header, ...rows].join("\n");
}

/**
 * Serialise energy transactions as XML.
 */
export function energyTransactionsToXml(transactions: EnergyTransaction[]): string {
  const items = transactions
    .map((tx) => {
      const metadata = tx.metadata
        ? `<metadata>${escapeXml(JSON.stringify(tx.metadata))}</metadata>`
        : "";
      return (
        "  <transaction>" +
        `<timestamp>${escapeXml(tx.timestamp)}</timestamp>` +
        `<meterId>${escapeXml(tx.meterId)}</meterId>` +
        `<type>${escapeXml(tx.type)}</type>` +
        `<amountKwh>${escapeXml(tx.amountKwh)}</amountKwh>` +
        (tx.value !== undefined ? `<value>${escapeXml(tx.value)}</value>` : "") +
        metadata +
        "</transaction>"
      );
    })
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<energyExport>\n${items}\n</energyExport>`;
}

/**
 * Serialise energy transactions in the requested format.
 *
 * Closes #905 — supports CSV, JSON, and XML output for GDPR data exports.
 */
export function serializeEnergyExport(
  transactions: EnergyTransaction[],
  format: ExportFormat,
): string {
  switch (format) {
    case "csv":
      return energyTransactionsToCsv(transactions);
    case "xml":
      return energyTransactionsToXml(transactions);
    case "json":
    default:
      return JSON.stringify({ transactions }, null, 2);
  }
}
