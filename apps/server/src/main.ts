import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildApp } from "./app.js";

const value = process.env.PORT ?? "8080";
if (!/^\d+$/u.test(value) || Number(value) > 65_535) throw new Error("PORT must be a valid TCP port");
const app = await buildApp({ publicDir: resolve(fileURLToPath(new URL("../../../apps/web/dist", import.meta.url))), logger: true });
const shutdown = async (): Promise<void> => { const forced = setTimeout(() => process.exit(1), 10_000).unref(); await app.close(); clearTimeout(forced); };
process.once("SIGINT", () => { void shutdown(); }); process.once("SIGTERM", () => { void shutdown(); });
await app.listen({ host: "0.0.0.0", port: Number(value) });
