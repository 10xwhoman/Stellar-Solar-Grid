import { env } from "@/lib/env";

const API_BASE = env.NEXT_PUBLIC_COLLAB_API_URL;

export interface CollaboratorShare {
  address: string;
  basisPoints: number;
}

/**
 * Fetches all collaborators and their shares in a single request.
 * The backend resolves this with one get_all_shares simulation — no N+1.
 */
export async function getCollaborators(): Promise<CollaboratorShare[]> {
  const res = await fetch(`${API_BASE}/collaborators`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch collaborators");
  }
  const { collaborators } = (await res.json()) as { collaborators: CollaboratorShare[] };
  return collaborators;
}

export async function addCollaborator(address: string, basisPoints: number): Promise<void> {
  const res = await fetch(`${API_BASE}/collaborators`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ address, basis_points: basisPoints }),
  });
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to add collaborator");
  }
}

export async function removeCollaborator(address: string): Promise<void> {
  const res = await fetch(`${API_BASE}/collaborators/${address}`, {
    method: "DELETE",
  });
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to remove collaborator");
  }
}

export interface PaymentRecord {
  transactionHash: string;
  date: string;
  meterId: string;
  amount: number;
  paymentPlan: string;
  status: string;
}

export interface PaymentHistoryFilters {
  startDate?: string;
  endDate?: string;
  sortBy?: "date" | "amount" | "status";
  sortOrder?: "asc" | "desc";
}

export async function getPaymentHistory(filters: PaymentHistoryFilters = {}): Promise<PaymentRecord[]> {
  const params = new URLSearchParams();
  if (filters.startDate) params.set("startDate", filters.startDate);
  if (filters.endDate) params.set("endDate", filters.endDate);
  if (filters.sortBy) params.set("sortBy", filters.sortBy);
  if (filters.sortOrder) params.set("sortOrder", filters.sortOrder);
  const queryString = params.toString();
  const res = await fetch(`${API_BASE}/payments${queryString ? `?${queryString}` : ""}`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch payment history");
  }
  const { payments } = (await res.json()) as { payments: PaymentRecord[] };
  return payments;
}

export interface ForecastDay {
  date: string;
  predictedUsageKwh: number;
  predictedSolarKwh: number;
  confidenceLowKwh: number;
  confidenceHighKwh: number;
  temperatureC: number;
  sunlightHours: number;
}

export interface MeterForecast {
  meterId: string;
  generatedAt: string;
  days: ForecastDay[];
  creditRunoutDate: string | null;
  creditRunoutAlert: boolean;
}

export interface ForecastOptions {
  days?: number;
  includeSolar?: boolean;
}

export async function fetchMeterForecast(
  meterId: string,
  options: ForecastOptions = {},
): Promise<MeterForecast> {
  const params = new URLSearchParams();
  if (options.days !== undefined) params.set("days", String(options.days));
  if (options.includeSolar !== undefined) params.set("includeSolar", String(options.includeSolar));
  const queryString = params.toString();
  const res = await fetch(
    `${API_BASE}/meters/${meterId}/forecast${queryString ? `?${queryString}` : ""}`,
  );
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch meter forecast");
  }
  return (await res.json()) as MeterForecast;
}

export interface ApplianceUsage {
  applianceId: string;
  name: string;
  category: string;
  usageKwh: number;
  sharePercent: number;
  trendPercent: number;
  recommendation: string;
}

export interface ApplianceBreakdown {
  meterId: string;
  periodStart: string;
  periodEnd: string;
  totalUsageKwh: number;
  appliances: ApplianceUsage[];
}

export async function fetchApplianceBreakdown(meterId: string): Promise<ApplianceBreakdown> {
  const res = await fetch(`${API_BASE}/meters/${meterId}/appliances`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch appliance breakdown");
  }
  return (await res.json()) as ApplianceBreakdown;
}

export type DiagnosticCommand = "reboot" | "self-test" | "check-connectivity";

export interface DiagnosticResult {
  id: string;
  meterId: string;
  command: DiagnosticCommand;
  status: "pending" | "running" | "passed" | "failed";
  startedAt: string;
  completedAt: string | null;
  message: string;
  suggestedFix: string | null;
}

export async function triggerDiagnostic(
  meterId: string,
  command: DiagnosticCommand,
): Promise<DiagnosticResult> {
  const res = await fetch(`${API_BASE}/meters/${meterId}/diagnostics`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command }),
  });
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to trigger diagnostic");
  }
  return (await res.json()) as DiagnosticResult;
}

export async function fetchDiagnosticLogs(meterId: string): Promise<DiagnosticResult[]> {
  const res = await fetch(`${API_BASE}/meters/${meterId}/diagnostic-logs`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch diagnostic logs");
  }
  const { logs } = (await res.json()) as { logs: DiagnosticResult[] };
  return logs;
}

export interface SolarFarm {
  id: string;
  name: string;
  location: string;
  capacityKw: number;
  availableShares: number;
  pricePerShare: number;
  totalGenerationKwh: number;
}

export interface SolarShare {
  farmId: string;
  shares: number;
  creditsKwh: number;
  purchasedAt: string;
}

export async function listSolarFarms(): Promise<SolarFarm[]> {
  const res = await fetch(`${API_BASE}/solar-farms`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch solar farms");
  }
  const { farms } = (await res.json()) as { farms: SolarFarm[] };
  return farms;
}

export async function buySolarShare(farmId: string, shares: number): Promise<SolarShare> {
  const res = await fetch(`${API_BASE}/solar-farms/${farmId}/shares`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ shares }),
  });
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to purchase solar share");
  }
  return (await res.json()) as SolarShare;
}

export async function getMyShares(): Promise<SolarShare[]> {
  const res = await fetch(`${API_BASE}/solar-farms/shares/me`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch solar shares");
  }
  const { shares } = (await res.json()) as { shares: SolarShare[] };
  return shares;
}
