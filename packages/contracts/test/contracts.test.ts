import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  ContractValidationError,
  decodeErrorResponse,
  decodeHealthResponse,
  decodePlanInput,
  decodePlanResponse,
  decodeProjectionResponse,
  parseJsonWithBigInts,
  stringifyJsonWithBigInts,
} from "../src/index.js";

const fixtures = new URL("./fixtures/v1/", import.meta.url);
const fixtureNames = [
  "health.json",
  "create-plan.json",
  "update-plan.json",
  "stale-update.json",
  "stored-projection.json",
  "stateless-projection.json",
  "repeated-delete.json",
];

function fixture(name: string): string {
  return readFileSync(new URL(name, fixtures), "utf8");
}

describe("frozen v1 contracts", () => {
  it("parses every copied golden fixture", () => {
    for (const name of fixtureNames) expect(() => parseJsonWithBigInts(fixture(name))).not.toThrow();
  });

  it("decodes every explicit public response", () => {
    const create = decodePlanResponse(fixture("create-plan.json"));
    const update = decodePlanResponse(fixture("update-plan.json"));
    const stale = decodeErrorResponse(fixture("stale-update.json"));
    const health = decodeHealthResponse(fixture("health.json"));
    const stored = decodeProjectionResponse(fixture("stored-projection.json"));
    const stateless = decodeProjectionResponse(fixture("stateless-projection.json"));
    expect(create.revision).toBe(1n);
    expect(update.revision).toBe(2n);
    expect(stale.code).toBe("REVISION_CONFLICT");
    expect(health.status).toBe("ok");
    expect(stored.plan_ref).not.toBeNull();
    expect(stateless.plan_ref).toBeNull();
    expect(stored.years).toEqual(stateless.years);
  });

  it("preserves the golden stateless projection bytes", () => {
    const source = fixture("stateless-projection.json").trim();
    expect(stringifyJsonWithBigInts(decodeProjectionResponse(source))).toBe(source);
  });

  it("rejects unknown mutation fields at every input level", () => {
    expect(() => decodePlanInput('{"profile":{"current_age":40,"current_annual_income_cents":0,"projection_years":1},"accounts":[],"misspelled_income":1}')).toThrow(ContractValidationError);
    expect(() => decodePlanInput('{"profile":{"current_age":40,"current_annual_income_cents":0,"projection_years":1,"extra":1},"accounts":[]}')).toThrow(/unknown field/u);
  });
});

describe("lossless HTTP integer codec", () => {
  it("round trips signed-64-bit extrema as unquoted JSON numbers", () => {
    const source = '{"minimum":-9223372036854775808,"maximum":9223372036854775807}';
    const decoded = parseJsonWithBigInts(source);
    expect(decoded).toEqual({ minimum: -(1n << 63n), maximum: (1n << 63n) - 1n });
    expect(stringifyJsonWithBigInts(decoded)).toBe(source);
  });

  it("never accepts strings, fractions, or out-of-range integers for currency", () => {
    const request = (income: string): string => `{"profile":{"current_age":40,"current_annual_income_cents":${income},"projection_years":1},"accounts":[]}`;
    for (const invalid of ['"9007199254740993"', "1.0", "9223372036854775808", "-9223372036854775809"]) {
      expect(() => decodePlanInput(request(invalid))).toThrow(/signed 64-bit JSON integer/u);
    }
    expect(decodePlanInput(request("9007199254740993")).profile.current_annual_income_cents).toBe(9_007_199_254_740_993n);
  });
});
