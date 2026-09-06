import type { RecommendBody, RoutePlan, Scenario, ScenarioCard, SimReport } from "./engine";

const BASE = (import.meta.env.VITE_MICP_API as string | undefined) ?? "";

export class EngineUnreachableError extends Error {
  constructor(message = "micp-api unreachable") {
    super(message);
    this.name = "EngineUnreachableError";
  }
}

export class EngineHttpError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, body: unknown) {
    super(typeof body === "object" && body && "error" in body ? String((body as { error: unknown }).error) : `HTTP ${status}`);
    this.status = status;
    this.body = body;
    this.name = "EngineHttpError";
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${BASE}${path}`, {
      ...init,
      headers: {
        Accept: "application/json",
        ...(init?.body ? { "Content-Type": "application/json" } : {}),
        ...init?.headers,
      },
    });
  } catch (err) {
    throw new EngineUnreachableError(err instanceof Error ? err.message : "fetch failed");
  }
  const text = await response.text();
  let payload: unknown = null;
  if (text) {
    try {
      payload = JSON.parse(text) as unknown;
    } catch {
      payload = { error: text };
    }
  }
  if (!response.ok) {
    throw new EngineHttpError(response.status, payload);
  }
  return payload as T;
}

export async function getHealth(): Promise<boolean> {
  try {
    const body = await request<{ status: string; service: string }>("/health");
    return body.status === "ok";
  } catch {
    return false;
  }
}

export async function listScenarios(): Promise<ScenarioCard[]> {
  return request<ScenarioCard[]>("/v1/scenarios");
}

export async function getScenario(id: string): Promise<Scenario> {
  return request<Scenario>(`/v1/scenarios/${encodeURIComponent(id)}`);
}

export async function evaluate(
  scenario: Scenario,
  allowInfeasible = true,
): Promise<RoutePlan> {
  return request<RoutePlan>("/v1/evaluate", {
    method: "POST",
    body: JSON.stringify({ scenario, allow_infeasible: allowInfeasible }),
  });
}

export async function recommend(
  scenario: Scenario,
  allowInfeasible = true,
): Promise<RecommendBody> {
  return request<RecommendBody>("/v1/recommend", {
    method: "POST",
    body: JSON.stringify({ scenario, allow_infeasible: allowInfeasible }),
  });
}

export async function simulate(
  scenario: Scenario,
  modelId: string,
  opts?: { seed?: number; durationS?: number; longTail?: boolean },
): Promise<SimReport> {
  return request<SimReport>("/v1/simulate", {
    method: "POST",
    body: JSON.stringify({
      scenario,
      model_id: modelId,
      seed: opts?.seed ?? 1,
      duration_s: opts?.durationS ?? 2.0,
      long_tail: opts?.longTail ?? false,
    }),
  });
}
