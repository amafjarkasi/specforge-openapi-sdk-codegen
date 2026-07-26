/**
 * Example TypeScript consumer of a specforge-generated Petstore SDK.
 *
 *   ./scripts/generate-examples.sh
 *   cd examples/petstore-ts && npm i && npx tsx main.mts
 */
import { createClient } from "./sdk/src/index.ts";

const baseUrl = process.env.PETSTORE_URL ?? "https://petstore3.swagger.io/api/v3";

const client = createClient({
  baseUrl,
  timeoutMs: 10_000,
  maxConcurrent: 4,
  retry: { maxRetries: 2 },
});

// Middleware: log every call
// (ApiClient is also usable directly if you need .use())
const pets = await client.pets.listPets({ limit: 5 }).catch((e: unknown) => {
  console.error("listPets failed (is the server up?)", e);
  return [] as unknown[];
});

console.log(`fetched ${Array.isArray(pets) ? pets.length : "?"} pets from ${baseUrl}`);
if (Array.isArray(pets)) {
  for (const p of pets.slice(0, 3)) {
    console.log("-", (p as { name?: string }).name ?? p);
  }
}
