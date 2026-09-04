import { describe, expect, test } from "bun:test";
import { issueLease, leasePayload, publicKeyFromSeed } from "../src/domain/lease";
import type { ClassroomLease } from "../src/domain/lease";

/**
 * Тестовый вектор, общий с Rust: тот же вектор проверяется в
 * `crates/transport/src/lease.rs::cross_language_test_vector_matches_cloud`.
 * Если хоть одна сторона изменит кодирование, тесты разойдутся до того, как
 * это увидит реальное устройство.
 */
const SEED = new Uint8Array(32).fill(7);

const LEASE: ClassroomLease = {
  teacherId: "teacher-1",
  organizationId: "org-1",
  branchId: "branch-1",
  allowedRooms: ["room-2", "room-3"],
  permissions: ["view_classroom", "control_classroom"],
  issuedAtUnixMs: 1000,
  expiresAtUnixMs: 100000,
};

describe("classroom lease", () => {
  test("payload имеет ожидаемую длину и начинается с версии", () => {
    const payload = leasePayload(LEASE);
    expect(payload[0]).toBe(1);
    // 1 + (4+9) + (4+5) + (4+8) + 4 + 2*(4+6) + 4 + (4+14) + (4+17) + 8 + 8
    expect(payload.length).toBe(1 + 13 + 9 + 12 + 4 + 20 + 4 + 18 + 21 + 16);
  });

  test("подпись детерминирована", () => {
    expect(issueLease(SEED, LEASE).signature).toBe(issueLease(SEED, LEASE).signature);
  });

  test("публичный ключ выводится из seed", () => {
    expect(publicKeyFromSeed(SEED).length).toBe(32);
  });

  test("изменение любого поля меняет подпись", () => {
    const base = issueLease(SEED, LEASE).signature;
    expect(issueLease(SEED, { ...LEASE, branchId: "branch-2" }).signature).not.toBe(base);
    expect(issueLease(SEED, { ...LEASE, allowedRooms: ["room-2"] }).signature).not.toBe(base);
    expect(issueLease(SEED, { ...LEASE, expiresAtUnixMs: 200000 }).signature).not.toBe(base);
  });
});

test("вектор для сверки с Rust", () => {
  const signed = issueLease(SEED, LEASE);
  console.log("lease signature:", signed.signature);
  console.log("issuer public key:", Buffer.from(publicKeyFromSeed(SEED)).toString("hex"));
  expect(signed.signature.length).toBe(128);
});
