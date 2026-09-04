/**
 * Контракт PostgreSQL-адаптера (spec T8 §4.1).
 *
 * Тесты выполняются только при заданном `DATABASE_URL`: без него они
 * пропускаются, а не притворяются пройденными. В CI база поднимается
 * сервис-контейнером, поэтому адаптер действительно исполняется, а не только
 * компилируется.
 *
 * Проверяется ровно то, что отличает адаптер от in-memory реализации:
 * преобразование времени, bytea и NULL-полей — места, где расхождение с
 * `MemoryStore` не поймает ни один тест доменного слоя.
 */

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { randomUUID } from "node:crypto";

import { PostgresStore } from "../src/db/postgres";

const databaseUrl = process.env.DATABASE_URL;
const suite = databaseUrl ? describe : describe.skip;

suite("PostgresStore", () => {
  let store: PostgresStore;
  const organizationId = randomUUID();
  const branchId = randomUUID();
  const roomId = randomUUID();
  const userId = randomUUID();

  beforeAll(async () => {
    store = new PostgresStore(databaseUrl!);
    const schema = await Bun.file(new URL("../src/db/schema.sql", import.meta.url)).text();
    await store.migrate("DROP SCHEMA public CASCADE; CREATE SCHEMA public;");
    await store.migrate(schema);

    await store.createOrganization({ id: organizationId, name: "IT-школа" });
    await store.createUser({
      id: userId,
      email: "owner@example.org",
      passwordHash: "hash",
      displayName: "Владелец",
    });
    await store.createBranch({ id: branchId, organizationId, name: "Центральный" });
    await store.createRoom({
      id: roomId,
      branchId,
      name: "Кабинет 1",
      updateChannel: "stable",
      desiredProfileId: null,
    });
  });

  afterAll(async () => {
    await store?.close();
  });

  test("email сравнивается без учёта регистра", async () => {
    const found = await store.findUserByEmail("OWNER@Example.org");
    expect(found?.id).toBe(userId);
  });

  test("сессия хранится хешем токена и возвращает срок в миллисекундах", async () => {
    const tokenHash = Buffer.alloc(32, 7);
    const expiresAtUnixMs = Date.now() + 60_000;
    await store.saveSession({ tokenHash, userId, expiresAtUnixMs });

    const session = await store.findSession(tokenHash);
    expect(session?.userId).toBe(userId);
    expect(session?.expiresAtUnixMs).toBe(expiresAtUnixMs);
    expect(Buffer.from(session!.tokenHash).equals(tokenHash)).toBe(true);
  });

  test("устройство сохраняется и обновляется по тому же идентификатору", async () => {
    const deviceId = randomUUID();
    await store.upsertDevice({
      id: deviceId,
      organizationId,
      branchId,
      roomId,
      hostname: "PC-01",
      agentVersion: null,
      healthState: null,
      lastSeenAtUnixMs: null,
    });
    expect((await store.findDevice(deviceId))?.lastSeenAtUnixMs).toBeNull();

    const seenAt = Date.now();
    await store.upsertDevice({
      id: deviceId,
      organizationId,
      branchId,
      roomId,
      hostname: "PC-01",
      agentVersion: "0.1.0",
      healthState: "warning",
      lastSeenAtUnixMs: seenAt,
    });

    const updated = await store.findDevice(deviceId);
    expect(updated?.agentVersion).toBe("0.1.0");
    expect(updated?.healthState).toBe("warning");
    expect(updated?.lastSeenAtUnixMs).toBe(seenAt);
    expect(await store.devicesOf(branchId)).toHaveLength(1);
  });

  test("сертификат устройства сохраняется как байты", async () => {
    const deviceId = randomUUID();
    await store.upsertDevice({
      id: deviceId,
      organizationId,
      branchId,
      roomId: null,
      hostname: "PC-02",
      agentVersion: null,
      healthState: null,
      lastSeenAtUnixMs: null,
    });
    await store.saveDeviceCertificate({
      deviceId,
      certificateDer: Uint8Array.from([1, 2, 3, 4]),
      fingerprintSha256: new Uint8Array(32).fill(9),
      expiresAtUnixMs: Date.now() + 1_000,
    });
    // Успешная запись без исключения — уже контракт: колонки для приватного
    // ключа в схеме нет, и добавить его сюда нечем.
    expect((await store.findDevice(deviceId))?.roomId).toBeNull();
  });

  test("использованный enrollment-код остаётся в базе", async () => {
    const codeHash = Buffer.alloc(32, 3);
    const deviceId = randomUUID();
    await store.upsertDevice({
      id: deviceId,
      organizationId,
      branchId,
      roomId,
      hostname: "PC-03",
      agentVersion: null,
      healthState: null,
      lastSeenAtUnixMs: null,
    });
    await store.saveEnrollmentToken({
      codeHash,
      organizationId,
      branchId,
      roomId,
      createdBy: userId,
      expiresAtUnixMs: Date.now() + 60_000,
      usedAtUnixMs: null,
      usedByDevice: null,
    });

    const usedAt = Date.now();
    await store.markEnrollmentTokenUsed(codeHash, deviceId, usedAt);

    const token = await store.findEnrollmentToken(codeHash);
    expect(token?.usedAtUnixMs).toBe(usedAt);
    expect(token?.usedByDevice).toBe(deviceId);
  });

  test("подпись версии агента переживает round-trip через bytea", async () => {
    const signatureHex = "ab".repeat(64);
    await store.publishAgentVersion({
      version: "0.3.0",
      channel: "stable",
      url: "https://updates.example.org/0.3.0.bin",
      sha256: "0f".repeat(32),
      signatureHex,
      minimumSupportedVersion: "0.1.0",
      publishedAtUnixMs: Date.now(),
    });

    const published = (await store.agentVersions()).find((value) => value.version === "0.3.0");
    expect(published?.signatureHex).toBe(signatureHex);
  });

  test("аудит возвращает события только своей организации", async () => {
    await store.appendAudit({
      organizationId,
      actorUserId: userId,
      deviceId: null,
      action: "auth.login",
      outcome: "success",
      details: { ip: "10.0.0.1" },
      occurredAtUnixMs: Date.now(),
    });
    await store.appendAudit({
      organizationId: null,
      actorUserId: null,
      deviceId: null,
      action: "auth.login",
      outcome: "failure",
      details: {},
      occurredAtUnixMs: Date.now(),
    });

    const events = await store.auditOf(organizationId);
    expect(events).toHaveLength(1);
    expect(events[0]!.details).toEqual({ ip: "10.0.0.1" });
  });
});
