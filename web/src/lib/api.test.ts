import { afterEach, describe, expect, it, vi } from "vitest";
import { EngineHttpError, EngineUnreachableError, evaluate, getHealth } from "./api";
import type { Scenario } from "./engine";

const SCENARIO = { id: "interactive_assistant" } as unknown as Scenario;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("api client", () => {
  it("treats network failure as engine unreachable", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.reject(new TypeError("Failed to fetch"))),
    );
    await expect(evaluate(SCENARIO)).rejects.toBeInstanceOf(EngineUnreachableError);
    expect(await getHealth()).toBe(false);
  });

  it("surfaces HTTP errors without inventing a plan", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve(
          new Response(JSON.stringify({ error: "no feasible model for the given constraints" }), {
            status: 409,
            headers: { "Content-Type": "application/json" },
          }),
        ),
      ),
    );
    await expect(evaluate(SCENARIO, false)).rejects.toBeInstanceOf(EngineHttpError);
  });
});
