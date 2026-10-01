import { RepositoryError } from "@retirement-calculator/application";
import type { PlanRepository } from "@retirement-calculator/application";
import type { Account, PlanProfile, RetirementPlan } from "@retirement-calculator/domain";

function snapshot(plan: RetirementPlan): RetirementPlan { return structuredClone(plan); }

export class MemoryPlanRepository implements PlanRepository {
  readonly #plans = new Map<string, RetirementPlan>();
  #available = true;

  setAvailable(available: boolean): void { this.#available = available; }
  #assertAvailable(): void { if (!this.#available) throw new RepositoryError("unavailable"); }

  create(plan: RetirementPlan): RetirementPlan {
    this.#assertAvailable();
    if (this.#plans.has(plan.id)) throw new RepositoryError("already_exists");
    this.#plans.set(plan.id, snapshot(plan));
    return snapshot(plan);
  }

  update(id: string, expectedRevision: bigint, profile: PlanProfile, accounts: Account[], updatedAt: Date): RetirementPlan {
    this.#assertAvailable();
    const current = this.#plans.get(id);
    if (current === undefined) throw new RepositoryError("not_found");
    if (current.revision !== expectedRevision) throw new RepositoryError("revision_conflict", current.revision);
    const next = snapshot({ ...current, revision: current.revision + 1n, profile, accounts, updatedAt });
    this.#plans.set(id, next);
    return snapshot(next);
  }

  get(id: string): RetirementPlan {
    this.#assertAvailable();
    const plan = this.#plans.get(id);
    if (plan === undefined) throw new RepositoryError("not_found");
    return snapshot(plan);
  }

  list(): RetirementPlan[] {
    this.#assertAvailable();
    return [...this.#plans.values()].map(snapshot).sort((left, right) =>
      left.createdAt.getTime() - right.createdAt.getTime() || left.id.localeCompare(right.id));
  }

  delete(id: string): void { this.#assertAvailable(); this.#plans.delete(id); }
  count(): number { this.#assertAvailable(); return this.#plans.size; }
  ready(): void { this.#assertAvailable(); }
}
