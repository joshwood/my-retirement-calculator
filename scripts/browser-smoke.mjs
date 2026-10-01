import { mkdir } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SMOKE_BASE_URL ?? "http://127.0.0.1:8080";
const artifacts = process.env.PAPERCLIP_RUN_SCRATCH_DIR ?? "test-results/browser-smoke";
await mkdir(artifacts, { recursive: true });

for (let attempt = 1; attempt <= 50; attempt += 1) {
  try {
    const response = await fetch(`${baseUrl}/api/v1/health/live`);
    if (response.ok) break;
  } catch {
    if (attempt === 50) throw new Error(`server did not become ready at ${baseUrl}`);
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
}

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
const browserErrors = [];
context.on("page", (currentPage) => currentPage.on("pageerror", (error) => browserErrors.push(error.message)));
const page = await context.newPage();

let createdPlan;
page.on("response", async (response) => {
  if (response.request().method() === "POST" && response.url().endsWith("/api/v1/plans") && response.status() === 201) {
    createdPlan = await response.json();
  }
});

await page.goto(baseUrl, { waitUntil: "networkidle" });
await page.getByRole("heading", { name: "Build a retirement scenario you can inspect." }).waitFor();
await page.getByRole("button", { name: "Save plan" }).waitFor({ state: "visible" });
const accountTypes = await page.locator("#account-0-type option").evaluateAll((options) => options.map((option) => option.value));
if (accountTypes.join(",") !== "traditional_ira,roth_ira,brokerage,employer_401k,cash,other") throw new Error("account type options are incomplete");
if (await page.locator("input:not([type=checkbox]), select").evaluateAll((fields) => fields.some((field) => !field.labels?.length))) throw new Error("an editable field has no semantic label");
await page.locator("#account-0-type").selectOption("other");
await page.getByLabel("Other account type").fill("Pension trust");
await page.locator("#account-0-type").selectOption("roth_ira");
await page.locator("#current-age").fill("40");
await page.locator("#annual-income").fill("100000");
await page.locator("#projection-years").fill("3");
await page.locator("#account-0-name").fill("Roth IRA");
await page.locator("#account-0-type").selectOption("roth_ira");
await page.locator("#account-0-balance").fill("100000");
await page.locator("#account-0-basis").fill("70000");
await page.locator("#account-0-growth").fill("6");
await page.locator("#account-0-yield").fill("5");
await page.locator("#account-0-allocation").fill("10");
await page.getByRole("button", { name: "+ Add account" }).click();
await page.locator("#account-1-name").fill("Brokerage");
await page.locator("#account-1-type").selectOption("brokerage");
await page.locator("#account-1-balance").fill("25000");
await page.locator("#account-1-basis").fill("20000");
await page.locator("#account-1-growth").fill("4");
await page.locator("#account-1-yield").fill("2");
await page.locator("#account-1-allocation").fill("5");
await page.locator('[data-account-index="1"]').getByRole("button", { name: "Duplicate" }).click();
if ((await page.locator(".account-card").count()) !== 3) throw new Error("duplicate did not create an ordered account card");
await page.getByRole("button", { name: "Remove account 3" }).click();
if ((await page.locator(".account-card").count()) !== 2) throw new Error("remove did not update the ordered account cards");

await page.getByRole("button", { name: "Project retirement" }).click();
await page.getByRole("heading", { name: "Your outlook" }).waitFor();
if ((await page.locator("table tbody tr").count()) !== 3) throw new Error("annual table is incomplete");
if ((await page.locator(".result-card").count()) !== 4) throw new Error("summary cards are incomplete");

await page.locator("#current-age").fill("10");
await page.getByRole("button", { name: "Project retirement" }).click();
await page.getByText("Must be between 18 and 100").waitFor();
if ((await page.locator("#account-1-name").inputValue()) !== "Brokerage") throw new Error("validation discarded form state");
await page.locator("#current-age").fill("40");

await page.getByRole("button", { name: "Save plan" }).click();
await page.getByText("Plan saved in this local service session.").waitFor();
if (!createdPlan) throw new Error("save response was not observed");
const updateBody = {
  expected_revision: createdPlan.revision,
  profile: createdPlan.profile,
  accounts: createdPlan.accounts,
};
const concurrent = await fetch(`${baseUrl}/api/v1/plans/${createdPlan.plan_id}`, {
  method: "PUT",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(updateBody),
});
if (!concurrent.ok) throw new Error(`could not arrange stale write: ${concurrent.status}`);
await page.locator("#account-0-name").fill("Roth IRA edited");
await page.getByRole("button", { name: "Save changes" }).click();
await page.getByText("A newer saved version exists").waitFor();
if ((await page.locator("#account-0-name").inputValue()) !== "Roth IRA edited") throw new Error("stale write discarded edits");
await page.getByRole("button", { name: "Reapply my changes" }).click();
await page.getByText("Plan saved in this local service session.").waitFor();

await fetch(`${baseUrl}/api/v1/plans/${createdPlan.plan_id}`, { method: "DELETE" });
await page.getByRole("button", { name: "Save changes" }).click();
await page.getByText("Saved plan is missing").waitFor();
await page.getByRole("button", { name: "Create replacement" }).click();
await page.getByText("Plan saved in this local service session.").waitFor();

await page.screenshot({ path: `${artifacts}/desktop-success.png`, fullPage: true });

await page.setViewportSize({ width: 320, height: 800 });
if ((await page.evaluate(() => document.documentElement.scrollWidth)) > 320) throw new Error("320px layout has horizontal page overflow");
await page.screenshot({ path: `${artifacts}/mobile-320.png`, fullPage: true });

const chartPage = await context.newPage();
await chartPage.setViewportSize({ width: 1024, height: 900 });
await chartPage.goto(`${baseUrl}/?chart=fail`, { waitUntil: "networkidle" });
await chartPage.locator("#account-0-name").fill("Cash reserve");
await chartPage.locator("#account-0-type").selectOption("cash");
await chartPage.locator("#account-0-balance").fill("10000");
await chartPage.getByRole("button", { name: "Project retirement" }).click();
await chartPage.getByText("Chart unavailable").waitFor();
await chartPage.locator("table").waitFor();
await chartPage.screenshot({ path: `${artifacts}/chart-failure.png`, fullPage: true });

const degradedPage = await context.newPage();
await degradedPage.route("**/api/v1/health/ready", (route) => route.fulfill({ status: 503, contentType: "application/json", body: JSON.stringify({ code: "SERVICE_UNAVAILABLE", message: "Service temporarily unavailable", field_errors: [], request_id: "smoke-degraded" }) }));
await degradedPage.goto(baseUrl, { waitUntil: "networkidle" });
await degradedPage.getByText("Saving temporarily unavailable").waitFor();
if (await degradedPage.getByRole("button", { name: "Save plan" }).isEnabled()) throw new Error("save remains enabled in degraded state");

const restartPage = await context.newPage();
await restartPage.goto(baseUrl, { waitUntil: "networkidle" });
await restartPage.locator("#account-0-name").fill("Preserve me");
await restartPage.route("**/api/v1/health/live", (route) => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ status: "ok", build_version: "smoke", instance_id: "smoke-restarted-instance" }) }));
await restartPage.getByRole("button", { name: "Project retirement" }).click();
await restartPage.getByText("Service memory was reset").waitFor();
if ((await restartPage.locator("#account-0-name").inputValue()) !== "Preserve me") throw new Error("restart discarded form state");

const keyboardPage = await context.newPage();
await keyboardPage.goto(baseUrl, { waitUntil: "networkidle" });
await keyboardPage.keyboard.press("Tab");
if (!(await keyboardPage.evaluate(() => document.activeElement?.classList.contains("skip-link")))) throw new Error("skip link is not first in keyboard order");
const skipOutline = await keyboardPage.locator(".skip-link").evaluate((link) => getComputedStyle(link).outlineStyle);
if (skipOutline === "none") throw new Error("keyboard focus is not visibly indicated");
await keyboardPage.keyboard.press("Enter");
if ((await keyboardPage.evaluate(() => location.hash)) !== "#planner") throw new Error("skip link did not target the planner");
await keyboardPage.keyboard.press("Tab");
if ((await keyboardPage.evaluate(() => document.activeElement?.id)) !== "current-age") throw new Error("keyboard order did not reach the first form field");
await keyboardPage.keyboard.type("45");

const emptyPage = await context.newPage();
await emptyPage.goto(baseUrl, { waitUntil: "networkidle" });
await emptyPage.getByRole("button", { name: "Remove account 1" }).click();
await emptyPage.getByRole("heading", { name: "No accounts yet" }).waitFor();
await emptyPage.getByRole("button", { name: "Add your first account" }).click();
if ((await emptyPage.locator(".account-card").count()) !== 1) throw new Error("empty-state account creation failed");

const errorPage = await context.newPage();
await errorPage.route("**/api/v1/projections", (route) => route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ code: "INTERNAL_ERROR", message: "Projection temporarily failed", field_errors: [], request_id: "smoke-server-error" }) }));
await errorPage.goto(baseUrl, { waitUntil: "networkidle" });
await errorPage.locator("#account-0-name").fill("Keep this draft");
await errorPage.getByRole("button", { name: "Project retirement" }).click();
await errorPage.getByText("Projection temporarily failed Request ID: smoke-server-error").waitFor();
if ((await errorPage.locator("#account-0-name").inputValue()) !== "Keep this draft") throw new Error("server error discarded form state");

if (browserErrors.length) throw new Error(`browser errors: ${browserErrors.join("; ")}`);
console.log(`browser smoke: PASS; artifacts=${artifacts}`);
await browser.close();
