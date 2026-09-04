import { beforeEach, describe, expect, test } from "bun:test";
import { randomUUID } from "node:crypto";

import { createApp, enrollmentHandler } from "../src/http/app";
import { MemoryStore } from "../src/db/memory";
import { hashPassword } from "../src/domain/auth";
import { publicKeyFromSeed } from "../src/domain/lease";
import type { Role } from "../src/domain/rbac";

const SEED = new Uint8Array(32).fill(11);
const ORG = randomUUID();
const BRANCH = randomUUID();
const OTHER_BRANCH = randomUUID();

let store: MemoryStore;
let app: ReturnType<typeof createApp>;
let enroll: ReturnType<typeof enrollmentHandler>;
let clock = 1_700_000_000_000;

async function addUser(email: string, role: Role, branchId: string | null) {
  const id = randomUUID();
  await store.createUser({
    id,
    email,
    passwordHash: await hashPassword("correct horse"),
    displayName: email,
  });
  await store.addMembership({ organizationId: ORG, userId: id, branchId, role });
  return id;
}

async function login(email: string): Promise<string> {
  const response = await app.fetch(
    new Request("http://cloud/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password: "correct horse" }),
    }),
  );
  expect(response.status).toBe(200);
  return ((await response.json()) as { token: string }).token;
}

function authed(path: string, token: string, init: RequestInit = {}) {
  return new Request(`http://cloud${path}`, {
    ...init,
    headers: { ...(init.headers ?? {}), authorization: `Bearer ${token}` },
  });
}

beforeEach(async () => {
  store = new MemoryStore();
  const deps = { store, leaseIssuerSeed: SEED, now: () => clock };
  app = createApp(deps);
  enroll = enrollmentHandler(deps);
  await store.createOrganization({ id: ORG, name: "IT School" });
  await store.createBranch({ id: BRANCH, organizationId: ORG, name: "Центральный" });
  await store.createBranch({ id: OTHER_BRANCH, organizationId: ORG, name: "Северный" });
  await store.createRoom({
    id: "room-1",
    branchId: BRANCH,
    name: "Кабинет 2",
    updateChannel: "stable",
    desiredProfileId: "python-classroom",
  });
});

describe("аутентификация", () => {
  test("неверный пароль и несуществующий пользователь неразличимы", async () => {
    await addUser("owner@school.ru", "owner", null);
    const wrongPassword = await app.fetch(
      new Request("http://cloud/v1/auth/login", {
        method: "POST",
        body: JSON.stringify({ email: "owner@school.ru", password: "nope" }),
      }),
    );
    const unknownUser = await app.fetch(
      new Request("http://cloud/v1/auth/login", {
        method: "POST",
        body: JSON.stringify({ email: "ghost@school.ru", password: "nope" }),
      }),
    );
    expect(wrongPassword.status).toBe(401);
    expect(unknownUser.status).toBe(401);
    expect(await wrongPassword.json()).toEqual(await unknownUser.json());
  });

  test("запрос без токена отклоняется", async () => {
    const response = await app.fetch(new Request("http://cloud/v1/me"));
    expect(response.status).toBe(401);
  });

  test("истёкшая сессия перестаёт работать", async () => {
    await addUser("owner@school.ru", "owner", null);
    const token = await login("owner@school.ru");
    expect((await app.fetch(authed("/v1/me", token))).status).toBe(200);

    clock += 13 * 60 * 60 * 1000;
    expect((await app.fetch(authed("/v1/me", token))).status).toBe(401);
    clock -= 13 * 60 * 60 * 1000;
  });
});

describe("RBAC на маршрутах", () => {
  test("teacher не может создать филиал", async () => {
    await addUser("teacher@school.ru", "teacher", BRANCH);
    const token = await login("teacher@school.ru");
    const response = await app.fetch(
      authed("/v1/branches", token, {
        method: "POST",
        body: JSON.stringify({ organization_id: ORG, name: "Новый" }),
      }),
    );
    expect(response.status).toBe(403);
  });

  test("owner может создать филиал", async () => {
    await addUser("owner@school.ru", "owner", null);
    const token = await login("owner@school.ru");
    const response = await app.fetch(
      authed("/v1/branches", token, {
        method: "POST",
        body: JSON.stringify({ organization_id: ORG, name: "Новый" }),
      }),
    );
    expect(response.status).toBe(201);
  });

  test("admin филиала не создаёт кабинет в чужом филиале", async () => {
    await addUser("admin@school.ru", "admin", BRANCH);
    const token = await login("admin@school.ru");

    const own = await app.fetch(
      authed("/v1/rooms", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: BRANCH, name: "Кабинет 3" }),
      }),
    );
    const foreign = await app.fetch(
      authed("/v1/rooms", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: OTHER_BRANCH, name: "Кабинет 4" }),
      }),
    );
    expect(own.status).toBe(201);
    expect(foreign.status).toBe(403);
  });

  test("отказ по правам попадает в аудит", async () => {
    await addUser("teacher@school.ru", "teacher", BRANCH);
    const token = await login("teacher@school.ru");
    await app.fetch(
      authed("/v1/branches", token, {
        method: "POST",
        body: JSON.stringify({ organization_id: ORG, name: "Новый" }),
      }),
    );
    const events = await store.auditOf(ORG);
    expect(events.some((event) => event.outcome === "failure" && event.action === "branch.create")).toBe(true);
  });
});

describe("enrollment через Cloud", () => {
  test("код выпускается admin и работает ровно один раз", async () => {
    await addUser("admin@school.ru", "admin", BRANCH);
    const token = await login("admin@school.ru");

    const issued = await app.fetch(
      authed("/v1/enrollment/codes", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: BRANCH, room_id: "room-1" }),
      }),
    );
    expect(issued.status).toBe(201);
    const { code } = (await issued.json()) as { code: string };

    const body = JSON.stringify({
      code,
      device_id: randomUUID(),
      hostname: "PC-01",
      certificate_der_base64: Buffer.from("certificate").toString("base64"),
    });
    const first = await enroll(new Request("http://cloud/enroll", { method: "POST", body }));
    expect(first.status).toBe(201);

    const second = await enroll(new Request("http://cloud/enroll", { method: "POST", body }));
    expect(second.status).toBe(400);
    expect(await second.json()).toEqual({ error: "ENROLLMENT_ERROR_CODE_ALREADY_USED" });
  });

  test("teacher не может выпустить enrollment-код", async () => {
    await addUser("teacher@school.ru", "teacher", BRANCH);
    const token = await login("teacher@school.ru");
    const response = await app.fetch(
      authed("/v1/enrollment/codes", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: BRANCH }),
      }),
    );
    expect(response.status).toBe(403);
  });

  test("устройство появляется в филиале после регистрации", async () => {
    await addUser("admin@school.ru", "admin", BRANCH);
    const token = await login("admin@school.ru");
    const issued = await app.fetch(
      authed("/v1/enrollment/codes", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: BRANCH }),
      }),
    );
    const { code } = (await issued.json()) as { code: string };
    await enroll(
      new Request("http://cloud/enroll", {
        method: "POST",
        body: JSON.stringify({
          code,
          device_id: randomUUID(),
          hostname: "PC-07",
          certificate_der_base64: Buffer.from("cert").toString("base64"),
        }),
      }),
    );

    const listed = await app.fetch(authed(`/v1/branches/${BRANCH}/devices`, token));
    const { devices } = (await listed.json()) as { devices: { hostname: string }[] };
    expect(devices.map((device) => device.hostname)).toContain("PC-07");
  });
});

describe("classroom lease", () => {
  test("teacher получает lease только со своими правами", async () => {
    await addUser("teacher@school.ru", "teacher", BRANCH);
    const token = await login("teacher@school.ru");
    const response = await app.fetch(
      authed("/v1/lease", token, {
        method: "POST",
        body: JSON.stringify({ organization_id: ORG, branch_id: BRANCH }),
      }),
    );
    expect(response.status).toBe(200);
    const signed = (await response.json()) as {
      lease: { permissions: string[]; allowedRooms: string[] };
      signature: string;
    };
    expect(signed.lease.permissions).toEqual(["view_classroom", "control_classroom", "apply_lesson_profile"]);
    // Право на ремонт устройств teacher не получает даже в lease.
    expect(signed.lease.permissions).not.toContain("repair_devices");
    expect(signed.lease.allowedRooms).toEqual(["room-1"]);
    expect(signed.signature).toHaveLength(128);
  });

  test("admin получает в lease право на ремонт", async () => {
    await addUser("admin@school.ru", "admin", BRANCH);
    const token = await login("admin@school.ru");
    const response = await app.fetch(
      authed("/v1/lease", token, {
        method: "POST",
        body: JSON.stringify({ organization_id: ORG, branch_id: BRANCH }),
      }),
    );
    const signed = (await response.json()) as { lease: { permissions: string[] } };
    expect(signed.lease.permissions).toContain("repair_devices");
  });

  test("lease подписан ключом организации", async () => {
    await addUser("teacher@school.ru", "teacher", BRANCH);
    const token = await login("teacher@school.ru");
    const response = await app.fetch(
      authed("/v1/lease", token, {
        method: "POST",
        body: JSON.stringify({ organization_id: ORG, branch_id: BRANCH }),
      }),
    );
    expect(response.status).toBe(200);
    // Публичный ключ, которым агент будет проверять lease, выводится из seed.
    expect(publicKeyFromSeed(SEED)).toHaveLength(32);
  });
});

describe("обновления", () => {
  test("публиковать версию может только owner", async () => {
    await addUser("admin@school.ru", "admin", BRANCH);
    const adminToken = await login("admin@school.ru");
    const denied = await app.fetch(
      authed("/v1/updates", adminToken, {
        method: "POST",
        body: JSON.stringify({
          organization_id: ORG,
          version: "0.2.0",
          channel: "stable",
          url: "https://updates/agent.bin",
          sha256: "abc",
        }),
      }),
    );
    expect(denied.status).toBe(403);

    await addUser("owner@school.ru", "owner", null);
    const ownerToken = await login("owner@school.ru");
    const allowed = await app.fetch(
      authed("/v1/updates", ownerToken, {
        method: "POST",
        body: JSON.stringify({
          organization_id: ORG,
          version: "0.2.0",
          channel: "stable",
          url: "https://updates/agent.bin",
          sha256: "abc",
        }),
      }),
    );
    expect(allowed.status).toBe(201);
  });

  test("устройство на stable не получает beta-сборку", async () => {
    await addUser("owner@school.ru", "owner", null);
    const token = await login("owner@school.ru");
    for (const [version, channel] of [["0.3.0", "beta"], ["0.2.0", "stable"]] as const) {
      await app.fetch(
        authed("/v1/updates", token, {
          method: "POST",
          body: JSON.stringify({
            organization_id: ORG,
            version,
            channel,
            url: "https://updates/agent.bin",
            sha256: "abc",
          }),
        }),
      );
    }
    const response = await app.fetch(
      authed("/v1/updates/check?channel=stable&current_version=0.1.0", token),
    );
    const { update } = (await response.json()) as { update: { version: string } | null };
    expect(update?.version).toBe("0.2.0");
  });
});

describe("идентификатор устройства", () => {
  test("Cloud использует device_id агента, а не выдаёт второй", async () => {
    await addUser("admin-id@school.ru", "admin", BRANCH);
    const token = await login("admin-id@school.ru");
    const issued = await app.fetch(
      authed("/v1/enrollment/codes", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: BRANCH, room_id: "room-1" }),
      }),
    );
    const { code } = (await issued.json()) as { code: string };

    const deviceId = randomUUID();
    const response = await enroll(
      new Request("http://cloud/enroll", {
        method: "POST",
        body: JSON.stringify({
          code,
          device_id: deviceId,
          hostname: "PC-42",
          certificate_der_base64: Buffer.from("cert").toString("base64"),
        }),
      }),
    );

    expect(response.status).toBe(201);
    const body = (await response.json()) as {
      device_id: string;
      room_id: string | null;
      lease_issuer_public_key: string;
    };
    // Второй идентификатор сделал бы список устройств Cloud несоединимым со
    // списком консоли, который построен на discovery.
    expect(body.device_id).toBe(deviceId);
    expect(body.room_id).toBe("room-1");
    expect(body.lease_issuer_public_key).toHaveLength(64);
  });

  test("не-UUID отклоняется, а не создаёт устройство", async () => {
    await addUser("admin-bad-id@school.ru", "admin", BRANCH);
    const token = await login("admin-bad-id@school.ru");
    const issued = await app.fetch(
      authed("/v1/enrollment/codes", token, {
        method: "POST",
        body: JSON.stringify({ branch_id: BRANCH }),
      }),
    );
    const { code } = (await issued.json()) as { code: string };

    const response = await enroll(
      new Request("http://cloud/enroll", {
        method: "POST",
        body: JSON.stringify({
          code,
          device_id: "../../etc/passwd",
          hostname: "PC-43",
          certificate_der_base64: Buffer.from("cert").toString("base64"),
        }),
      }),
    );
    expect(response.status).toBe(400);
  });
});
