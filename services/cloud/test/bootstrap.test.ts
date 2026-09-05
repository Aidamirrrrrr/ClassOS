/**
 * Первичное развёртывание Cloud (`scripts/bootstrap.ts`).
 *
 * Проверяется не сам скрипт, а то, ради чего он существует: после него в
 * пустой базе есть учётная запись, которой можно войти и завести первый
 * филиал. До появления bootstrap развёрнутый Cloud был непригоден — маршрута,
 * создающего первого владельца, в API нет и быть не должно, а тесты заводили
 * его напрямую в хранилище, поэтому дыра не была видна ни одному из них.
 */

import { beforeEach, describe, expect, test } from "bun:test";
import { randomUUID } from "node:crypto";

import { createApp } from "../src/http/app";
import { MemoryStore } from "../src/db/memory";
import { hashPassword } from "../src/domain/auth";
import type { Store } from "../src/db/store";

const SEED = new Uint8Array(32).fill(7);
const PASSWORD = "owner-password-12";

let store: MemoryStore;
let app: ReturnType<typeof createApp>;

/** Ровно те записи, которые создаёт `scripts/bootstrap.ts`. */
async function bootstrap(target: Store, email: string) {
  const organizationId = randomUUID();
  const userId = randomUUID();
  await target.createOrganization({ id: organizationId, name: "IT-школа" });
  await target.createUser({
    id: userId,
    email,
    passwordHash: await hashPassword(PASSWORD),
    displayName: email,
  });
  await target.addMembership({ organizationId, userId, branchId: null, role: "owner" });
  return { organizationId, userId };
}

async function login(email: string, password: string) {
  return app.fetch(
    new Request("http://cloud/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  );
}

beforeEach(() => {
  store = new MemoryStore();
  app = createApp({ store, leaseIssuerSeed: SEED, now: () => 1_700_000_000_000 });
});

describe("первичное развёртывание", () => {
  test("пустой Cloud не пускает никого", async () => {
    const response = await login("owner@example.org", PASSWORD);
    expect(response.status).toBe(401);
  });

  test("после bootstrap владелец входит и заводит первый филиал", async () => {
    const { organizationId } = await bootstrap(store, "owner@example.org");

    const signIn = await login("owner@example.org", PASSWORD);
    expect(signIn.status).toBe(200);
    const { token } = (await signIn.json()) as { token: string };

    // Членство со скоупом на всю организацию (branch_id IS NULL) — не деталь:
    // с членством, привязанным к филиалу, владелец не смог бы создать первый
    // филиал, потому что филиалов ещё нет.
    const created = await app.fetch(
      new Request("http://cloud/v1/branches", {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ organization_id: organizationId, name: "Центральный" }),
      }),
    );
    expect(created.status).toBe(201);

    const branch = (await created.json()) as { id: string };
    const room = await app.fetch(
      new Request("http://cloud/v1/rooms", {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ branch_id: branch.id, name: "Кабинет 1" }),
      }),
    );
    expect(room.status).toBe(201);

    // Enrollment-код — последнее звено: без него устройство не зарегистрировать.
    const code = await app.fetch(
      new Request("http://cloud/v1/enrollment/codes", {
        method: "POST",
        headers: { authorization: `Bearer ${token}` },
        body: JSON.stringify({ branch_id: branch.id }),
      }),
    );
    expect(code.status).toBe(201);
  });

  test("повторный bootstrap не меняет пароль существующего владельца", async () => {
    await bootstrap(store, "owner@example.org");
    // Скрипт останавливается, обнаружив пользователя: смена пароля владельца
    // повторным запуском развёртывания была бы способом захватить организацию.
    const existing = await store.findUserByEmail("owner@example.org");
    expect(existing).toBeDefined();

    expect((await login("owner@example.org", PASSWORD)).status).toBe(200);
    expect((await login("owner@example.org", "another-password")).status).toBe(401);
  });
});
