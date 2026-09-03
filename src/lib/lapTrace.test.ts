import { describe, expect, it } from "vitest";
import {
  COMPLETE_TRACE_COVERAGE,
  findPartialTraces,
  formatCoveragePct,
  isPartialTrace,
} from "./lapTrace";

describe("isPartialTrace", () => {
  it("passes a lap recorded end to end", () => {
    expect(isPartialTrace({ trace_coverage: 1 })).toBe(false);
    expect(isPartialTrace({ trace_coverage: COMPLETE_TRACE_COVERAGE })).toBe(
      false,
    );
  });

  it("catches a lap whose recording is a fragment", () => {
    expect(isPartialTrace({ trace_coverage: 0.5 })).toBe(true);
  });

  it("treats an unmeasured lap as whole", () => {
    // Recorded before the measure existed, with no trace left to backfill
    // from. A warning invented here would land on laps that are fine.
    expect(isPartialTrace({ trace_coverage: null })).toBe(false);
    expect(isPartialTrace({})).toBe(false);
  });
});

describe("findPartialTraces", () => {
  it("names only the laps that are fragments", () => {
    const partial = findPartialTraces([
      { id: "whole", trace_coverage: 0.999 },
      { id: "cut", trace_coverage: 1 },
      { id: "attached-late", trace_coverage: 0.42 },
      { id: "legacy" },
    ]);
    expect([...partial]).toEqual(["attached-late"]);
  });
});

describe("formatCoveragePct", () => {
  it("reads as a percentage of the lap", () => {
    expect(formatCoveragePct(0.482)).toBe("48 %");
    expect(formatCoveragePct(1)).toBe("100 %");
  });
});
