import { chromium } from "playwright";

const baseUrl = process.env.SMOKE_BASE_URL ?? "http://127.0.0.1:3000";

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
const page = await browser.newPage();
page.on("console", (message) => console.log(`browser console: ${message.text()}`));
page.on("pageerror", (error) => console.error(`browser error: ${error.message}`));
await page.goto(baseUrl, { waitUntil: "networkidle" });
await page.locator("#health-status").waitFor({ timeout: 10_000 });
const status = await page.locator("#health-status").textContent();
if (status !== "health fixture decoded: ok") {
  throw new Error(`unexpected browser status: ${status}`);
}
console.log(`browser smoke: PASS (${status})`);
await browser.close();
