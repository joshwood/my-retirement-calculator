import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { App } from "../src/App.js";
import { initialDraft, toInput } from "../src/model.js";

describe("React workflow", () => {
  it("renders an accessible labelled workflow and draft-preserving controls", () => {
    const html = renderToStaticMarkup(<App />);
    expect(html).toContain('href="#planner"'); expect(html).toContain('for="current-age"'); expect(html).toContain("Project retirement"); expect(html).toContain("Save plan"); expect(html).toContain("Remove account 1");
  });
  it("converts editable decimals losslessly without mutating the draft", () => {
    const draft = initialDraft(); draft.profile.income = "90071992547409.93"; const before = structuredClone(draft);
    expect(toInput(draft).profile.current_annual_income_cents).toBe(9_007_199_254_740_993n); expect(draft).toEqual(before);
  });
});
