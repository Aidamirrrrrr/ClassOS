/**
 * Точка входа Cloud v0.
 *
 * PostgreSQL-адаптер появляется вместе с реальным развёртыванием; сейчас
 * сервис поднимается на in-memory хранилище, что честно отражено в ответе
 * `/health` и в README милестоуна.
 */

import { createApp, enrollmentHandler } from "./http/app";
import { MemoryStore } from "./db/memory";

const store = new MemoryStore();
const leaseIssuerSeed = new Uint8Array(32);
crypto.getRandomValues(leaseIssuerSeed);

const app = createApp({ store, leaseIssuerSeed, now: () => Date.now() });
const enroll = enrollmentHandler({ store, leaseIssuerSeed, now: () => Date.now() });

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
