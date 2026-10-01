import { describe, expect, it } from "vitest";
import { decodeErrorResponse, decodeHealthResponse, decodePlanResponse, decodeProjectionResponse, stringifyJsonWithBigInts } from "@retirement-calculator/contracts";
import { MemoryPlanRepository } from "@retirement-calculator/adapters-memory";
import { buildApp } from "../src/app.js";

const accountId = "10000000-0000-4000-8000-000000000001";
const planId = "20000000-0000-4000-8000-000000000001";
const input = { profile: { current_age: 40, current_annual_income_cents: 10_000_000n, projection_years: 2 }, accounts: [{ id: accountId, name: "Roth", account_type: "roth_ira" as const, other_type_label: null, starting_balance_cents: 10_000_000n, cost_basis_cents: 7_000_000n, annual_growth_bps: 600, annual_dividend_yield_bps: 500, contribution_allocation_bps: 1000, reinvest_dividends: true }] };
const json = (value: unknown) => ({ headers: { "content-type": "application/json" }, payload: stringifyJsonWithBigInts(value) });

describe("Fastify v1 API", () => {
  it("serves create/get/update/delete and stored/stateless projection parity", async () => {
    const app = await buildApp({ generateId: () => planId, clock: () => new Date("2026-01-01T00:00:00Z"), instanceId: "instance-a" });
    const createdResponse = await app.inject({ method: "POST", url: "/api/v1/plans", ...json(input) });
    expect(createdResponse.statusCode).toBe(201); const created = decodePlanResponse(createdResponse.body); expect(created.revision).toBe(1n);
    expect(decodePlanResponse((await app.inject({ method: "GET", url: `/api/v1/plans/${planId}` })).body)).toEqual(created);
    const stateless = decodeProjectionResponse((await app.inject({ method: "POST", url: "/api/v1/projections", ...json(input) })).body);
    const stored = decodeProjectionResponse((await app.inject({ method: "POST", url: `/api/v1/plans/${planId}/projections`, ...json({}) })).body);
    expect(stored.years).toEqual(stateless.years); expect(stored.plan_ref).toEqual({ plan_id: planId, revision: 1n }); expect(stateless.plan_ref).toBeNull();
    const updated = decodePlanResponse((await app.inject({ method: "PUT", url: `/api/v1/plans/${planId}`, ...json({ expected_revision: 1n, ...input }) })).body); expect(updated.revision).toBe(2n);
    const stale = await app.inject({ method: "PUT", url: `/api/v1/plans/${planId}`, ...json({ expected_revision: 1n, ...input }) }); expect(stale.statusCode).toBe(409); expect(decodeErrorResponse(stale.body).code).toBe("REVISION_CONFLICT");
    expect((await app.inject({ method: "DELETE", url: `/api/v1/plans/${planId}` })).statusCode).toBe(204); expect((await app.inject({ method: "DELETE", url: `/api/v1/plans/${planId}` })).statusCode).toBe(204);
    await app.close();
  });

  it("normalizes malformed, unknown, invalid UUID, and oversize requests", async () => {
    const app = await buildApp();
    const malformed = await app.inject({ method: "POST", url: "/api/v1/plans", headers: { "content-type": "application/json" }, payload: "{" }); expect(decodeErrorResponse(malformed.body).code).toBe("INVALID_JSON");
    const unknown = await app.inject({ method: "POST", url: "/api/v1/plans", ...json({ ...input, surprise: 1n }) }); expect(decodeErrorResponse(unknown.body)).toMatchObject({ code: "VALIDATION_ERROR", field_errors: [{ code: "UNKNOWN_FIELD" }] });
    const invalid = await app.inject({ method: "GET", url: "/api/v1/plans/not-a-uuid" }); expect(decodeErrorResponse(invalid.body)).toMatchObject({ code: "VALIDATION_ERROR", field_errors: [{ path: "plan_id", code: "INVALID_FORMAT" }] });
    const oversize = await app.inject({ method: "POST", url: "/api/v1/plans", headers: { "content-type": "application/json" }, payload: `{"padding":"${"x".repeat(1024 * 1024)}"}` }); expect(oversize.statusCode).toBe(413); expect(decodeErrorResponse(oversize.body).code).toBe("REQUEST_TOO_LARGE");
    await app.close();
  });

  it("separates liveness/readiness, emits identity, headers, request IDs and metrics", async () => {
    const repository = new MemoryPlanRepository(); const app = await buildApp({ repository, instanceId: "instance-a" });
    const live = await app.inject({ method: "GET", url: "/health" }); expect(decodeHealthResponse(live.body).instance_id).toBe("instance-a"); expect(live.headers["x-request-id"]).toBeTruthy();
    expect(live.headers).toMatchObject({ "x-content-type-options": "nosniff", "referrer-policy": "no-referrer" });
    repository.setAvailable(false); expect((await app.inject({ method: "GET", url: "/api/v1/health/live" })).statusCode).toBe(200); expect((await app.inject({ method: "GET", url: "/api/v1/health/ready" })).statusCode).toBe(503);
    const metrics = await app.inject({ method: "GET", url: "/metrics" }); expect(metrics.body).toContain("retirement_http_requests_total"); expect(metrics.headers["content-type"]).toContain("version=0.0.4");
    await app.close();
  });
});
