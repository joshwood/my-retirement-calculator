import { describe, expect, it } from "vitest";
import type { PlanRepository } from "../src/index.js";
import { Application } from "../src/index.js";
import type { RetirementPlan } from "@retirement-calculator/domain";

const input = { profile: { currentAge: 40, currentAnnualIncomeCents: 100_000n, projectionYears: 1 }, accounts: [] };

describe("Application", () => {
  it("owns injected identity and time", () => {
    let stored: RetirementPlan | undefined;
    const repository: PlanRepository = {
      create: (plan) => (stored = structuredClone(plan)), update: () => { throw new Error("unused"); },
      get: () => { if (stored === undefined) throw new Error("missing"); return structuredClone(stored); },
      list: () => [], delete: () => undefined, count: () => 1, ready: () => undefined,
    };
    const now = new Date("2026-01-02T03:04:05.000Z");
    const app = new Application({ repository, clock: () => now, generateId: () => "plan-id" });
    expect(app.create(input)).toMatchObject({ id: "plan-id", revision: 1n, createdAt: now, updatedAt: now });
  });
});
