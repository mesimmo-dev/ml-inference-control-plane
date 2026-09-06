import { describe, expect, it } from "vitest";
import { SAMPLE_FLEET, SAMPLE_WEIGHTS } from "./types";
import { scoreCandidate } from "./score";

describe("scoreCandidate", () => {
  it("ranks the fast model above the 70b under balanced-ish default weights", () => {
    const fast = SAMPLE_FLEET.find((m) => m.id === "llama-8b-fast")!;
    const quality = SAMPLE_FLEET.find((m) => m.id === "llama-70b-quality")!;
    expect(scoreCandidate(fast, SAMPLE_WEIGHTS, 10)).toBeGreaterThan(
      scoreCandidate(quality, SAMPLE_WEIGHTS, 10),
    );
  });

  it("is finite and in (0, 1] for the sample fleet", () => {
    for (const model of SAMPLE_FLEET) {
      const s = scoreCandidate(model, SAMPLE_WEIGHTS, 10);
      expect(Number.isFinite(s)).toBe(true);
      expect(s).toBeGreaterThan(0);
      expect(s).toBeLessThanOrEqual(1);
    }
  });
});
