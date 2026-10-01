import { describe, expect, it } from "vitest";
import { RepositoryError } from "@retirement-calculator/application";
import type { RetirementPlan } from "@retirement-calculator/domain";
import { MemoryPlanRepository } from "../src/index.js";

function plan(id: string, age = 40): RetirementPlan {
  return { id, revision: 1n, profile: { currentAge: age, currentAnnualIncomeCents: 100_000n, projectionYears: 1 }, accounts: [], createdAt: new Date(0), updatedAt: new Date(0) };
}

describe("MemoryPlanRepository", () => {
  it("returns isolated snapshots and stable ordering", () => {
    const repository = new MemoryPlanRepository();
    repository.create(plan("b")); repository.create(plan("a"));
    const read = repository.get("a"); read.profile.currentAge = 99;
    expect(repository.get("a").profile.currentAge).toBe(40);
    expect(repository.list().map(({ id }) => id)).toEqual(["a", "b"]);
  });

  it("atomically admits one expected-revision update and never mutates on conflict", async () => {
    const repository = new MemoryPlanRepository(); repository.create(plan("a"));
    const update = (age: number) => Promise.resolve().then(() => repository.update("a", 1n, plan("a", age).profile, [], new Date(age)));
    const results = await Promise.allSettled([update(41), update(42)]);
    expect(results.filter(({ status }) => status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected" && result.reason instanceof RepositoryError && result.reason.code === "revision_conflict")).toHaveLength(1);
    expect(repository.get("a").revision).toBe(2n);
  });

  it("deletes idempotently and separates readiness/count", () => {
    const repository = new MemoryPlanRepository(); repository.delete("missing"); expect(repository.count()).toBe(0);
    repository.setAvailable(false); expect(() => { repository.ready(); }).toThrow(RepositoryError);
  });
});
