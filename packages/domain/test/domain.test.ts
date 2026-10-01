import { describe, expect, it } from "vitest";
import {
  CalculationError,
  ValidationErrors,
  applyBps,
  project,
  projectRetirementPlan,
  validateForSave,
  type Account,
  type PlanProfile,
} from "../src/index.js";

function profile(projectionYears = 1): PlanProfile {
  return { currentAge: 40, currentAnnualIncomeCents: 10_000_000n, projectionYears };
}

function account(id = "00000000-0000-0000-0000-000000000001"): Account {
  return {
    id,
    name: "Roth IRA",
    accountType: "roth_ira",
    otherTypeLabel: null,
    startingBalanceCents: 10_000_000n,
    costBasisCents: 7_000_000n,
    annualGrowthBps: 600,
    annualDividendYieldBps: 500,
    contributionAllocationBps: 1_000,
    reinvestDividends: true,
  };
}

describe("validation", () => {
  it("allows empty saved drafts but rejects empty projections", () => {
    expect(() => { validateForSave(profile(), []); }).not.toThrow();
    expect(() => project(profile(), [])).toThrow(ValidationErrors);
    try {
      project(profile(), []);
    } catch (error) {
      expect((error as ValidationErrors).errors).toContainEqual(expect.objectContaining({ path: "accounts", code: "required" }));
    }
  });

  it("returns aggregate, duplicate, and field errors in one pass", () => {
    const first = { ...account(), name: " ", contributionAllocationBps: 1 };
    const second = { ...account(), contributionAllocationBps: 10_000 };
    try {
      validateForSave(profile(), [first, second]);
      throw new Error("expected validation failure");
    } catch (error) {
      expect(error).toBeInstanceOf(ValidationErrors);
      const errors = (error as ValidationErrors).errors;
      expect(errors).toEqual(expect.arrayContaining([
        expect.objectContaining({ path: "accounts[0].name", code: "required" }),
        expect.objectContaining({ path: "accounts[1].id", code: "duplicate" }),
        expect.objectContaining({ path: "accounts.contribution_allocation_bps", code: "aggregate_out_of_range" }),
      ]));
    }
  });

  it("counts Unicode scalar values and enforces the account cap", () => {
    expect(() => { validateForSave(profile(), [{ ...account(), name: "🦀".repeat(80) }]); }).not.toThrow();
    expect(() => { validateForSave(profile(), [{ ...account(), name: `${"🦀".repeat(80)}x` }]); }).toThrow(ValidationErrors);
    expect(() => { validateForSave(profile(), Array.from({ length: 101 }, (_, index) => account(String(index)))); }).toThrow(ValidationErrors);
  });
});

describe("checked deterministic projection", () => {
  it("rounds each midpoint away from zero independently and carries the year", () => {
    expect(applyBps(1n, 5_000)).toBe(1n);
    expect(applyBps(-1n, 5_000)).toBe(-1n);
    const input = {
      ...account(),
      startingBalanceCents: 1n,
      costBasisCents: 99n,
      contributionAllocationBps: 5_000,
      annualGrowthBps: 5_000,
      annualDividendYieldBps: 5_000,
    };
    const output = project({ currentAge: 40, currentAnnualIncomeCents: 1n, projectionYears: 2 }, [input]);
    expect(output.years[0]?.accounts[0]).toEqual(expect.objectContaining({
      openingBalanceCents: 1n,
      contributionCents: 1n,
      investedBalanceCents: 2n,
      appreciationCents: 1n,
      dividendGeneratedCents: 1n,
      closingBalanceCents: 4n,
    }));
    expect(output.years[1]?.accounts[0]?.openingBalanceCents).toBe(4n);
    expect(output.years[1]?.age).toBe(42);
  });

  it("preserves input order and sums every portfolio field", () => {
    const second = { ...account("2"), name: "Brokerage", accountType: "brokerage" as const, contributionAllocationBps: 0, startingBalanceCents: 0n };
    const output = project(profile(), [account("1"), second]);
    const year = output.years[0];
    expect(year?.accounts.map((row) => row.accountId)).toEqual(["1", "2"]);
    expect(output.accounts.map((entry) => entry.id)).toEqual(["1", "2"]);
    expect(year?.portfolio.closingBalanceCents).toBe(year?.accounts.reduce((total, row) => total + row.closingBalanceCents, 0n));
  });

  it("implements the zero floor and separate dividend payout policy", () => {
    const floor = project(profile(), [{ ...account(), contributionAllocationBps: 0, annualGrowthBps: -10_000 }]);
    const floorRow = floor.years[0]?.accounts[0];
    expect((floorRow?.investedBalanceCents ?? 0n) + (floorRow?.appreciationCents ?? 0n)).toBe(0n);
    expect(floorRow?.closingBalanceCents).toBe(floorRow?.dividendReinvestedCents);
    const payout = project(profile(2), [{ ...account(), contributionAllocationBps: 0, annualGrowthBps: 0, reinvestDividends: false }]);
    expect(payout.years[0]?.accounts[0]).toEqual(expect.objectContaining({ dividendReinvestedCents: 0n, incomePaidCents: 500_000n }));
    expect(payout.years[1]?.accounts[0]?.cumulativeIncomePaidCents).toBe(1_000_000n);
  });

  it("throws on account, portfolio, and cumulative overflow without a partial value", () => {
    const max = (1n << 63n) - 1n;
    expect(() => project(profile(), [{ ...account(), startingBalanceCents: max, contributionAllocationBps: 1, annualGrowthBps: 0, annualDividendYieldBps: 0 }])).toThrow(CalculationError);
    const large = { ...account("1"), startingBalanceCents: max / 2n + 1n, contributionAllocationBps: 0, annualGrowthBps: 0, annualDividendYieldBps: 0 };
    expect(() => project(profile(), [large, { ...large, id: "2" }])).toThrow(CalculationError);
    const payout = { ...large, annualDividendYieldBps: 10_000, reinvestDividends: false };
    expect(() => project(profile(2), [payout])).toThrow(CalculationError);
  });

  it("uses identical stored and stateless domain-boundary models", () => {
    const stateless = project(profile(2), [account()]);
    const stored = projectRetirementPlan({
      id: "plan",
      revision: 1n,
      profile: profile(2),
      accounts: [account()],
      createdAt: new Date(0),
      updatedAt: new Date(0),
    });
    expect(stored).toEqual(stateless);
  });
});
