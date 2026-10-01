import { randomUUID } from "node:crypto";
import { join } from "node:path";
import fastifyStatic from "@fastify/static";
import Fastify from "fastify";
import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import { MemoryPlanRepository } from "@retirement-calculator/adapters-memory";
import { Application, RepositoryError } from "@retirement-calculator/application";
import { ContractValidationError, CONTRACT_VERSION, decodePlanInput, decodeStoredProjectionInput, decodeUpdatePlanInput, stringifyJsonWithBigInts } from "@retirement-calculator/contracts";
import type { ErrorResponse, FieldError, PlanInput, PlanResponse, ProjectionResponse } from "@retirement-calculator/contracts";
import { CalculationError, ValidationErrors } from "@retirement-calculator/domain";
import type { Account, RetirementPlan } from "@retirement-calculator/domain";

const MAX_BODY_BYTES = 1024 * 1024;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;
const SECURITY_HEADERS = {
  "content-security-policy": "default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'",
  "referrer-policy": "no-referrer",
  "x-content-type-options": "nosniff",
} as const;

interface Metrics { requests: number; requestErrors: number; durationMs: number; projections: number; projectionFailures: number; projectionDurationMs: number }
export interface BuildAppOptions { repository?: MemoryPlanRepository; clock?: () => Date; generateId?: () => string; instanceId?: string; publicDir?: string; logger?: boolean }

export async function buildApp(options: BuildAppOptions = {}): Promise<FastifyInstance> {
  const repository = options.repository ?? new MemoryPlanRepository();
  const application = new Application({ repository, clock: options.clock ?? (() => new Date()), generateId: options.generateId ?? randomUUID });
  const health = { status: "ok", build_version: "0.1.0", instance_id: options.instanceId ?? randomUUID() };
  const metrics: Metrics = { requests: 0, requestErrors: 0, durationMs: 0, projections: 0, projectionFailures: 0, projectionDurationMs: 0 };
  const requestStarts = new WeakMap<object, number>();
  const app = Fastify({ logger: options.logger ?? false, bodyLimit: MAX_BODY_BYTES, genReqId: () => randomUUID(), disableRequestLogging: true });
  app.removeContentTypeParser("application/json");
  app.addContentTypeParser("application/json", { parseAs: "string" }, (_request, body, done) => done(null, body));

  app.addHook("onRequest", async (request, reply) => { requestStarts.set(request, performance.now()); reply.header("x-request-id", request.id); for (const [name, value] of Object.entries(SECURITY_HEADERS)) reply.header(name, value); });
  app.addHook("onResponse", async (request, reply) => {
    const started = requestStarts.get(request);
    const latency = started === undefined ? 0 : performance.now() - started;
    metrics.requests++; metrics.durationMs += latency; if (reply.statusCode >= 400) metrics.requestErrors++;
    request.log.info({ event: "request_completed", method: request.method, route: request.routeOptions.url, status: reply.statusCode, latency_ms: latency, request_id: request.id });
  });

  const body = <T>(request: FastifyRequest, decoder: (source: string) => T): T => {
    if (typeof request.body !== "string") throw new ContractValidationError("$", "expected JSON body");
    return decoder(request.body);
  };
  const sendJson = (reply: FastifyReply, value: unknown, status = 200): FastifyReply => reply.code(status).type("application/json; charset=utf-8").send(stringifyJsonWithBigInts(value));
  const parseId = (value: string): string => { if (!UUID.test(value)) throw new InvalidPlanId(); return value.toLowerCase(); };

  app.post("/api/v1/plans", async (request, reply) => {
    const plan = application.create(toDomain(body(request, decodePlanInput)));
    return sendJson(reply, toPlanResponse(plan), 201);
  });
  app.get<{ Params: { plan_id: string } }>("/api/v1/plans/:plan_id", async (request, reply) => sendJson(reply, toPlanResponse(application.get(parseId(request.params.plan_id)))));
  app.put<{ Params: { plan_id: string } }>("/api/v1/plans/:plan_id", async (request, reply) => {
    const input = body(request, decodeUpdatePlanInput);
    return sendJson(reply, toPlanResponse(application.update(parseId(request.params.plan_id), input.expected_revision, toDomain(input))));
  });
  app.delete<{ Params: { plan_id: string } }>("/api/v1/plans/:plan_id", async (request, reply) => { application.delete(parseId(request.params.plan_id)); return reply.code(204).send(); });
  app.post<{ Params: { plan_id: string } }>("/api/v1/plans/:plan_id/projections", async (request, reply) => {
    body(request, decodeStoredProjectionInput); return project(reply, () => application.projectStored(parseId(request.params.plan_id)), metrics);
  });
  app.post("/api/v1/projections", async (request, reply) => project(reply, () => application.projectStateless(toDomain(body(request, decodePlanInput))), metrics));
  app.get("/health", async (_request, reply) => sendJson(reply, health));
  app.get("/api/v1/health/live", async (_request, reply) => sendJson(reply, health));
  app.get("/api/v1/health/ready", async (_request, reply) => { repository.ready(); return sendJson(reply, health); });
  app.get("/metrics", async (_request, reply) => {
    let plans = 0; try { plans = repository.count(); } catch { /* readiness reports repository failure */ }
    const seconds = (value: number) => (value / 1000).toFixed(6);
    return reply.type("text/plain; version=0.0.4; charset=utf-8").send(`# TYPE retirement_http_requests_total counter\nretirement_http_requests_total ${metrics.requests}\n# TYPE retirement_http_request_errors_total counter\nretirement_http_request_errors_total ${metrics.requestErrors}\n# TYPE retirement_http_request_duration_seconds histogram\nretirement_http_request_duration_seconds_bucket{le="+Inf"} ${metrics.requests}\nretirement_http_request_duration_seconds_sum ${seconds(metrics.durationMs)}\nretirement_http_request_duration_seconds_count ${metrics.requests}\n# TYPE retirement_projections_total counter\nretirement_projections_total ${metrics.projections}\n# TYPE retirement_projection_failures_total counter\nretirement_projection_failures_total ${metrics.projectionFailures}\n# TYPE retirement_projection_duration_seconds histogram\nretirement_projection_duration_seconds_bucket{le="+Inf"} ${metrics.projections}\nretirement_projection_duration_seconds_sum ${seconds(metrics.projectionDurationMs)}\nretirement_projection_duration_seconds_count ${metrics.projections}\n# TYPE retirement_plans gauge\nretirement_plans ${plans}\n`);
  });

  if (options.publicDir !== undefined) await app.register(fastifyStatic, { root: options.publicDir, wildcard: false });
  app.setNotFoundHandler(async (request, reply) => {
    if (request.url.startsWith("/api/") || request.url === "/metrics") return sendError(reply, request.id, 404, "NOT_FOUND", "Route not found");
    if (options.publicDir === undefined) return sendError(reply, request.id, 404, "NOT_FOUND", "Route not found");
    return reply.type("text/html; charset=utf-8").sendFile("index.html", join(options.publicDir));
  });
  app.setErrorHandler((error, request, reply) => {
    if (reply.sent) return;
    if ((error as { code?: string }).code === "FST_ERR_CTP_BODY_TOO_LARGE") return sendError(reply, request.id, 413, "REQUEST_TOO_LARGE", "Request body exceeds the configured limit");
    if (error instanceof InvalidPlanId) return sendError(reply, request.id, 422, "VALIDATION_ERROR", "Request validation failed", [{ path: "plan_id", code: "INVALID_FORMAT", message: "Must be a UUID" }]);
    if (error instanceof ContractValidationError) {
      const unknown = error.message.includes("unknown field");
      const invalidUuid = error.message.includes("invalid UUID");
      const fieldError = unknown ? { path: error.path.replace(/^\$\.?/u, ""), code: "UNKNOWN_FIELD", message: "Field is not part of the v1 contract" } : { path: error.path.replace(/^\$\.?/u, ""), code: "INVALID_FORMAT", message: "Must be a UUID" };
      return sendError(reply, request.id, 422, unknown || invalidUuid ? "VALIDATION_ERROR" : "INVALID_JSON", unknown || invalidUuid ? "Request validation failed" : "Request body must match the v1 contract", unknown || invalidUuid ? [fieldError] : []);
    }
    if (error instanceof ValidationErrors) return sendError(reply, request.id, 422, "VALIDATION_ERROR", "Request validation failed", error.errors.map((entry) => ({ ...entry, code: entry.code.toUpperCase() })));
    if (error instanceof CalculationError) return sendError(reply, request.id, 422, "CALCULATION_RANGE_EXCEEDED", "Calculation range exceeded");
    if (error instanceof RepositoryError) {
      const map = { not_found: [404, "PLAN_NOT_FOUND", "Plan not found"], revision_conflict: [409, "REVISION_CONFLICT", "Plan revision conflict"], unavailable: [503, "SERVICE_UNAVAILABLE", "Service temporarily unavailable"], already_exists: [409, "PLAN_ALREADY_EXISTS", "Plan already exists"] } as const;
      const result = map[error.code]; return sendError(reply, request.id, result[0], result[1], result[2]);
    }
    if (error instanceof SyntaxError) return sendError(reply, request.id, 422, "INVALID_JSON", "Request body must match the v1 contract");
    request.log.error({ event: "request_failed", request_id: request.id, failure_code: "INTERNAL_ERROR" });
    return sendError(reply, request.id, 500, "INTERNAL_ERROR", "Internal server error");
  });
  return app;
}

class InvalidPlanId extends Error {}
function sendError(reply: FastifyReply, requestId: string, status: number, code: string, message: string, field_errors: FieldError[] = []): FastifyReply {
  const value: ErrorResponse = { code, message, field_errors, request_id: requestId };
  return reply.code(status).type("application/json; charset=utf-8").send(stringifyJsonWithBigInts(value));
}
function toDomain(input: PlanInput): { profile: RetirementPlan["profile"]; accounts: Account[] } {
  return { profile: { currentAge: input.profile.current_age, currentAnnualIncomeCents: input.profile.current_annual_income_cents, projectionYears: input.profile.projection_years }, accounts: input.accounts.map((account, index) => {
    if (!UUID.test(account.id)) throw new ContractValidationError(`$.accounts[${String(index)}].id`, "invalid UUID");
    return { id: account.id.toLowerCase(), name: account.name, accountType: account.account_type, otherTypeLabel: account.other_type_label, startingBalanceCents: account.starting_balance_cents, costBasisCents: account.cost_basis_cents, annualGrowthBps: account.annual_growth_bps, annualDividendYieldBps: account.annual_dividend_yield_bps, contributionAllocationBps: account.contribution_allocation_bps, reinvestDividends: account.reinvest_dividends };
  }) };
}
function accountDto(account: Account): PlanInput["accounts"][number] { return { id: account.id, name: account.name, account_type: account.accountType, other_type_label: account.otherTypeLabel, starting_balance_cents: account.startingBalanceCents, cost_basis_cents: account.costBasisCents, annual_growth_bps: account.annualGrowthBps, annual_dividend_yield_bps: account.annualDividendYieldBps, contribution_allocation_bps: account.contributionAllocationBps, reinvest_dividends: account.reinvestDividends }; }
function profileDto(plan: RetirementPlan): PlanInput["profile"] { return { current_age: plan.profile.currentAge, current_annual_income_cents: plan.profile.currentAnnualIncomeCents, projection_years: plan.profile.projectionYears }; }
function toPlanResponse(plan: RetirementPlan): PlanResponse { return { contract_version: CONTRACT_VERSION, plan_id: plan.id, revision: plan.revision, created_at: plan.createdAt.toISOString(), updated_at: plan.updatedAt.toISOString(), profile: profileDto(plan), accounts: plan.accounts.map(accountDto) }; }
function project(reply: FastifyReply, run: () => ReturnType<Application["projectStored"]>, metrics: Metrics): FastifyReply {
  const started = performance.now(); metrics.projections++;
  try {
    const result = run(); const source = result.storedPlan ?? { profile: result.projection.profile, accounts: result.projection.accounts };
    const response: ProjectionResponse = { contract_version: CONTRACT_VERSION, plan_ref: result.storedPlan === null ? null : { plan_id: result.storedPlan.id, revision: result.storedPlan.revision }, profile: result.storedPlan === null ? { current_age: source.profile.currentAge, current_annual_income_cents: source.profile.currentAnnualIncomeCents, projection_years: source.profile.projectionYears } : profileDto(result.storedPlan), accounts: source.accounts.map(accountDto), years: result.projection.years.map((year) => ({ year: year.year, age: year.age, accounts: year.accounts.map((row) => ({ account_id: row.accountId, opening_balance_cents: row.openingBalanceCents, contribution_cents: row.contributionCents, invested_balance_cents: row.investedBalanceCents, appreciation_cents: row.appreciationCents, dividend_generated_cents: row.dividendGeneratedCents, dividend_reinvested_cents: row.dividendReinvestedCents, income_paid_cents: row.incomePaidCents, closing_balance_cents: row.closingBalanceCents, cumulative_income_paid_cents: row.cumulativeIncomePaidCents })), portfolio: { opening_balance_cents: year.portfolio.openingBalanceCents, contribution_cents: year.portfolio.contributionCents, invested_balance_cents: year.portfolio.investedBalanceCents, appreciation_cents: year.portfolio.appreciationCents, dividend_generated_cents: year.portfolio.dividendGeneratedCents, dividend_reinvested_cents: year.portfolio.dividendReinvestedCents, income_paid_cents: year.portfolio.incomePaidCents, closing_balance_cents: year.portfolio.closingBalanceCents, cumulative_income_paid_cents: year.portfolio.cumulativeIncomePaidCents } })), warnings: [] };
    return reply.type("application/json; charset=utf-8").send(stringifyJsonWithBigInts(response));
  } catch (error) { metrics.projectionFailures++; throw error; } finally { metrics.projectionDurationMs += performance.now() - started; }
}
