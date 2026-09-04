/**
 * Точка входа Cloud v0.
 *
 * Хранилище выбирается по `DATABASE_URL`: с ним поднимается PostgreSQL, без
 * него — in-memory, что честно отражено в ответе `/health`.
 */

import { createApp, enrollmentHandler } from "./http/app";
import { MemoryStore } from "./db/memory";
import { PostgresStore } from "./db/postgres";
import type { Store } from "./db/store";

/**
 * Seed издателя classroom lease.
 *
 * Задаётся при развёртывании и переживает перезапуск: случайный ключ на
 * каждый старт обесценивал бы все выданные lease вместе с процессом, а
 * устройства, получившие публичный ключ при enrollment, перестали бы
 * принимать преподавателя (ADR-0016).
 */
function leaseIssuerSeed(): Uint8Array {
  const hex = process.env.CLASSOS_LEASE_ISSUER_SEED_HEX;
  if (!hex) {
    throw new Error(
      "CLASSOS_LEASE_ISSUER_SEED_HEX не задан: без постоянного ключа издателя " +
        "выданные lease перестают действовать при перезапуске Cloud",
    );
  }
  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw new Error("CLASSOS_LEASE_ISSUER_SEED_HEX должен быть 32 байтами в hex");
  }
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

const databaseUrl = process.env.DATABASE_URL;
const store: Store = databaseUrl ? new PostgresStore(databaseUrl) : new MemoryStore();
if (!databaseUrl) {
  console.warn("DATABASE_URL не задан — Cloud работает на in-memory хранилище, данные не переживут перезапуск.");
}

const seed = leaseIssuerSeed();
const app = createApp({ store, leaseIssuerSeed: seed, now: () => Date.now() });
const enroll = enrollmentHandler({ store, leaseIssuerSeed: seed, now: () => Date.now() });

const port = Number(process.env.PORT ?? 8787);

Bun.serve({
  port,
  fetch(request) {
    const url = new URL(request.url);
    if (request.method === "POST" && url.pathname === "/v1/enrollment/enroll") {
      return enroll(request);
    }
    return app.fetch(request);
  },
});

console.log(`ClassOS Cloud v0 слушает порт ${port}`);
