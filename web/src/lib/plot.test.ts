import { describe, expect, it } from "vitest";
import { extent, lognormalFromPercentiles, lognormalPdf, scale } from "./plot";

describe("plot helpers", () => {
  it("pads a degenerate domain", () => {
    const [lo, hi] = extent([5, 5]);
    expect(hi).toBeGreaterThan(lo);
  });

  it("maps domain to range", () => {
    expect(scale(5, 0, 10, 0, 100)).toBe(50);
  });

  it("reconstructs a lognormal from p50/p99 that peaks near p50", () => {
    const params = lognormalFromPercentiles(32.66, 57.57);
    expect(params).not.toBeNull();
    const { mu, sigma } = params!;
    const at50 = lognormalPdf(32.66, mu, sigma);
    const at99 = lognormalPdf(57.57, mu, sigma);
    expect(at50).toBeGreaterThan(at99);
  });
});
