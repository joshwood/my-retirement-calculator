import { decodeErrorResponse, decodeHealthResponse, decodePlanResponse, decodeProjectionResponse, stringifyJsonWithBigInts } from "@retirement-calculator/contracts";
import type { AccountInput, AccountType, ErrorResponse, PlanInput, PlanResponse, ProjectionResponse } from "@retirement-calculator/contracts";

export interface ProfileDraft { age: string; income: string; years: string }
export interface AccountDraft { id: string; name: string; type: AccountType; other: string; balance: string; basis: string; growth: string; dividend: string; allocation: string; reinvest: boolean }
export interface Draft { profile: ProfileDraft; accounts: AccountDraft[] }
export const blankAccount = (): AccountDraft => ({ id: crypto.randomUUID(), name: "", type: "traditional_ira", other: "", balance: "0", basis: "0", growth: "6", dividend: "0", allocation: "0", reinvest: true });
export const initialDraft = (): Draft => ({ profile: { age: "40", income: "100000", years: "25" }, accounts: [blankAccount()] });
const hundredths = (value: string): bigint => {
  const match = /^([+-]?)(\d*)(?:\.(\d{0,2}))?$/u.exec(value.trim()); if (match === null || (match[2] === "" && match[3] === undefined)) throw new Error("invalid decimal");
  const wholeToken = match[2] ?? ""; const whole = BigInt(wholeToken === "" ? "0" : wholeToken); const fraction = BigInt((match[3] ?? "").padEnd(2, "0")); return (match[1] === "-" ? -1n : 1n) * (whole * 100n + fraction);
};
export function toInput(draft: Draft): PlanInput {
  const account = (value: AccountDraft): AccountInput => ({ id: value.id, name: value.name, account_type: value.type, other_type_label: value.type === "other" ? value.other : null, starting_balance_cents: hundredths(value.balance), cost_basis_cents: hundredths(value.basis), annual_growth_bps: Number(hundredths(value.growth)), annual_dividend_yield_bps: Number(hundredths(value.dividend)), contribution_allocation_bps: Number(hundredths(value.allocation)), reinvest_dividends: value.reinvest });
  return { profile: { current_age: Number(draft.profile.age), current_annual_income_cents: hundredths(draft.profile.income), projection_years: Number(draft.profile.years) }, accounts: draft.accounts.map(account) };
}
export class ApiFailure extends Error { constructor(readonly status: number, readonly response: ErrorResponse) { super(response.message); } }
async function call<T>(path: string, init: RequestInit | undefined, decode: (source: string) => T): Promise<T> {
  const response = await fetch(path, init); const source = await response.text(); if (!response.ok) throw new ApiFailure(response.status, decodeErrorResponse(source)); return decode(source);
}
export const api = {
  live: () => call("/api/v1/health/live", undefined, decodeHealthResponse),
  ready: () => call("/api/v1/health/ready", undefined, decodeHealthResponse),
  get: (id: string) => call(`/api/v1/plans/${id}`, undefined, decodePlanResponse),
  create: (input: PlanInput) => call("/api/v1/plans", { method: "POST", headers: { "content-type": "application/json" }, body: stringifyJsonWithBigInts(input) }, decodePlanResponse),
  update: (plan: PlanResponse, input: PlanInput) => call(`/api/v1/plans/${plan.plan_id}`, { method: "PUT", headers: { "content-type": "application/json" }, body: stringifyJsonWithBigInts({ expected_revision: plan.revision, ...input }) }, decodePlanResponse),
  remove: async (id: string) => { const response = await fetch(`/api/v1/plans/${id}`, { method: "DELETE" }); if (!response.ok) throw new ApiFailure(response.status, decodeErrorResponse(await response.text())); },
  project: (input: PlanInput) => call("/api/v1/projections", { method: "POST", headers: { "content-type": "application/json" }, body: stringifyJsonWithBigInts(input) }, decodeProjectionResponse),
};
export const money = (value: bigint): string => new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(Number(value) / 100);
export type { PlanResponse, ProjectionResponse };
