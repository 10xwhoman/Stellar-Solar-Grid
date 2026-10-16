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

export type MaintenanceStatus = "scheduled" | "in_progress" | "completed" | "cancelled";

export interface MaintenanceWindow {
  id: string;
  title: string;
  description: string;
  startsAt: string;
  endsAt: string;
  affectedMeters: string[];
  status: MaintenanceStatus;
}

export async function getUpcomingMaintenance(): Promise<MaintenanceWindow[]> {
  const res = await fetch(`${API_BASE}/maintenance/upcoming`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch upcoming maintenance");
  }
  const { maintenance } = (await res.json()) as { maintenance: MaintenanceWindow[] };
  return maintenance;
}

export async function getActiveMaintenance(): Promise<MaintenanceWindow[]> {
  const res = await fetch(`${API_BASE}/maintenance/active`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch active maintenance");
  }
  const { maintenance } = (await res.json()) as { maintenance: MaintenanceWindow[] };
  return maintenance;
}

export async function getMaintenanceWindow(id: string): Promise<MaintenanceWindow> {
  const res = await fetch(`${API_BASE}/maintenance/${id}`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch maintenance window");
  }
  const { maintenance } = (await res.json()) as { maintenance: MaintenanceWindow };
  return maintenance;
}

export interface BatteryStats {
  voltageV: number;
  currentA: number;
  stateOfCharge: number;
  healthPct: number;
  cycleCount: number;
  predictedReplacementDate: string;
}

export async function getBatteryStats(meterId: string): Promise<BatteryStats> {
  const res = await fetch(`${API_BASE}/meters/${meterId}/battery`);
  if (!res.ok) {
    const { error } = (await res.json()) as { error: string };
    throw new Error(error ?? "Failed to fetch battery stats");
  }
  const { battery } = (await res.json()) as { battery: BatteryStats };
  return battery;
}
