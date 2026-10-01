import { project, validateForSave } from "@retirement-calculator/domain";
import type { Account, PlanProfile, Projection, RetirementPlan } from "@retirement-calculator/domain";

export type RepositoryErrorCode = "already_exists" | "not_found" | "revision_conflict" | "unavailable";

export class RepositoryError extends Error {
  constructor(readonly code: RepositoryErrorCode, readonly actualRevision?: bigint) {
    super(code);
  }
}

export interface PlanRepository {
  create(plan: RetirementPlan): RetirementPlan;
  update(id: string, expectedRevision: bigint, profile: PlanProfile, accounts: Account[], updatedAt: Date): RetirementPlan;
  get(id: string): RetirementPlan;
  list(): RetirementPlan[];
  delete(id: string): void;
  count(): number;
  ready(): void;
}

export interface ApplicationDependencies {
  repository: PlanRepository;
  clock: () => Date;
  generateId: () => string;
}

export interface PlanFields {
  profile: PlanProfile;
  accounts: Account[];
}

export interface ProjectedPlan {
  storedPlan: RetirementPlan | null;
  projection: Projection;
}

export class Application {
  readonly repository: PlanRepository;
  readonly #clock: () => Date;
  readonly #generateId: () => string;

  constructor({ repository, clock, generateId }: ApplicationDependencies) {
    this.repository = repository;
    this.#clock = clock;
    this.#generateId = generateId;
  }

  create(input: PlanFields): RetirementPlan {
    validateForSave(input.profile, input.accounts);
    const now = this.#clock();
    return this.repository.create({
      id: this.#generateId(), revision: 1n, profile: input.profile, accounts: input.accounts,
      createdAt: now, updatedAt: now,
    });
  }

  update(id: string, expectedRevision: bigint, input: PlanFields): RetirementPlan {
    validateForSave(input.profile, input.accounts);
    return this.repository.update(id, expectedRevision, input.profile, input.accounts, this.#clock());
  }

  get(id: string): RetirementPlan { return this.repository.get(id); }
  delete(id: string): void { this.repository.delete(id); }

  projectStateless(input: PlanFields): ProjectedPlan {
    return { storedPlan: null, projection: project(input.profile, input.accounts) };
  }

  projectStored(id: string): ProjectedPlan {
    const snapshot = this.repository.get(id);
    return { storedPlan: snapshot, projection: project(snapshot.profile, snapshot.accounts) };
  }
}
