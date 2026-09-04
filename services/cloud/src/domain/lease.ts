/**
 * Выпуск signed classroom lease (spec T8 §7).
 *
 * Формат байтов для подписи обязан **побайтово** совпадать с проверкой в
 * `crates/transport/src/lease.rs`: это кросс-языковой контракт, поэтому он
 * закреплён тестовым вектором с обеих сторон.
 */

import { createPrivateKey, createPublicKey, sign as nodeSign } from "node:crypto";

export const LEASE_VERSION = 1;

export const LEASE_PERMISSIONS = [
  "view_classroom",
  "control_classroom",
  "apply_lesson_profile",
  "repair_devices",
] as const;
export type LeasePermission = (typeof LEASE_PERMISSIONS)[number];

export interface ClassroomLease {
  readonly teacherId: string;
  readonly organizationId: string;
  readonly branchId: string;
  readonly allowedRooms: readonly string[];
  readonly permissions: readonly LeasePermission[];
  readonly issuedAtUnixMs: number;
  readonly expiresAtUnixMs: number;
}

class ByteWriter {
  private chunks: Uint8Array[] = [];

  byte(value: number): void {
    this.chunks.push(Uint8Array.of(value));
  }

  /** Поле с префиксом длины: без него границы полей неоднозначны. */
  lengthPrefixed(value: string): void {
    const bytes = new TextEncoder().encode(value);
    this.u32(bytes.length);
    this.chunks.push(bytes);
  }

  u32(value: number): void {
    const buffer = new Uint8Array(4);
    new DataView(buffer.buffer).setUint32(0, value, false);
    this.chunks.push(buffer);
  }

  i64(value: number): void {
    const buffer = new Uint8Array(8);
    new DataView(buffer.buffer).setBigInt64(0, BigInt(value), false);
    this.chunks.push(buffer);
  }

  finish(): Uint8Array {
    const total = this.chunks.reduce((sum, chunk) => sum + chunk.length, 0);
    const result = new Uint8Array(total);
    let offset = 0;
    for (const chunk of this.chunks) {
      result.set(chunk, offset);
      offset += chunk.length;
    }
    return result;
  }
}

export function leasePayload(lease: ClassroomLease): Uint8Array {
  const writer = new ByteWriter();
  writer.byte(LEASE_VERSION);
  writer.lengthPrefixed(lease.teacherId);
  writer.lengthPrefixed(lease.organizationId);
  writer.lengthPrefixed(lease.branchId);
  writer.u32(lease.allowedRooms.length);
  for (const room of lease.allowedRooms) writer.lengthPrefixed(room);
  writer.u32(lease.permissions.length);
  for (const permission of lease.permissions) writer.lengthPrefixed(permission);
  writer.i64(lease.issuedAtUnixMs);
  writer.i64(lease.expiresAtUnixMs);
  return writer.finish();
}

/** DER-обёртка PKCS#8 вокруг 32-байтного seed Ed25519. */
const PKCS8_ED25519_PREFIX = Uint8Array.from([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);

function privateKeyFromSeed(seed: Uint8Array) {
  if (seed.length !== 32) throw new Error("Ed25519 seed должен быть длиной 32 байта");
  const der = new Uint8Array(PKCS8_ED25519_PREFIX.length + seed.length);
  der.set(PKCS8_ED25519_PREFIX, 0);
  der.set(seed, PKCS8_ED25519_PREFIX.length);
  return createPrivateKey({ key: Buffer.from(der), format: "der", type: "pkcs8" });
}

/** Публичный ключ issuer в том же виде, в каком его ждёт агент: 32 сырых байта. */
export function publicKeyFromSeed(seed: Uint8Array): Uint8Array {
  const spki = createPublicKey(privateKeyFromSeed(seed)).export({
    format: "der",
    type: "spki",
  });
  return new Uint8Array(spki.subarray(spki.length - 32));
}

export interface SignedLease {
  readonly lease: ClassroomLease;
  /** Подпись в hex: агент получает её как 64 байта. */
  readonly signature: string;
}

/**
 * Подписывает lease ключом организации.
 *
 * Ключ живёт только в Cloud: устройство и Teacher Console его не видят и не
 * могут выпустить lease сами.
 */
export function issueLease(issuerSeed: Uint8Array, lease: ClassroomLease): SignedLease {
  const signature = nodeSign(null, Buffer.from(leasePayload(lease)), privateKeyFromSeed(issuerSeed));
  return { lease, signature: signature.toString("hex") };
}
