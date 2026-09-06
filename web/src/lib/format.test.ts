import { describe, expect, it } from "vitest";
import { formatMs, formatProb, formatUsd, neverEmpirical, originLabel } from "./format";
import { ORIGIN_COPY } from "./engine";

describe("formatters", () => {
  it("renders milliseconds and seconds", () => {
    expect(formatMs(57.566)).toContain("57.6");
    expect(formatMs(1500)).toContain("s");
    expect(formatMs(Number.NaN)).toBe("—");
  });

  it("does not present tiny probabilities as exact zeros when they are not", () => {
    expect(formatProb(0)).toBe("0");
    expect(formatProb(5.17e-14)).toBe("<1e-6");
  });

  it("formats sub-dollar costs", () => {
    expect(formatUsd(0.002048)).toBe("$0.0020");
  });
});

describe("origin labels", () => {
  it("uses the four required labels and never confuses modeled with empirical", () => {
    expect(originLabel("modeled")).toBe("MODELED");
    expect(originLabel("simulated")).toBe("SIMULATED");
    expect(originLabel("local_synthetic")).toBe("LOCAL SYNTHETIC BENCHMARK");
    expect(originLabel("empirical")).toBe("EMPIRICAL");
    expect(ORIGIN_COPY.modeled.note.toLowerCase()).toContain("not a measurement");
    expect(ORIGIN_COPY.simulated.note.toLowerCase()).toContain("not a production trace");
    expect(ORIGIN_COPY.empirical.note.toLowerCase()).toContain("unused");
    expect(neverEmpirical("modeled")).toBe(true);
    expect(neverEmpirical("simulated")).toBe(true);
  });
});
