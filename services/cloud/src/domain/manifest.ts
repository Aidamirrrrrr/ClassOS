/**
 * Подпись манифеста обновления (spec T8 §8.2).
 *
 * Канонические байты обязаны **побайтово** совпадать с `manifest_payload` в
 * `crates/updater/src/lib.rs`: подпись проверяется на устройстве, и
 * расхождение форматов означало бы, что ни одно обновление не установится.
 * Контракт закреплён тестовым вектором с обеих сторон.
 *
 * Приватный ключ издателя живёт только в среде выпуска релиза: ни Cloud в
 * рантайме, ни устройство его не видят.
 */

import { createPrivateKey, sign as nodeSign } from "node:crypto";

import type { UpdateManifest } from "./updates";

/** Поля с префиксом длины: иначе version и url можно переставить местами. */
export function manifestPayload(manifest: Omit<UpdateManifest, "signature">): Uint8Array {
  const encoder = new TextEncoder();
  const chunks: Uint8Array[] = [];
  for (const field of [
    manifest.version,
    manifest.url,
    manifest.sha256,
    manifest.minimum_supported_version,
    manifest.release_channel,
  ]) {
    const bytes = encoder.encode(field);
    const length = new Uint8Array(4);
    new DataView(length.buffer).setUint32(0, bytes.length, false);
    chunks.push(length, bytes);
  }
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const result = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

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

/** Подписывает манифест ключом издателя. */
export function signManifest(
  publisherSeed: Uint8Array,
  manifest: Omit<UpdateManifest, "signature">,
): UpdateManifest {
  const signature = nodeSign(null, Buffer.from(manifestPayload(manifest)), privateKeyFromSeed(publisherSeed));
  return { ...manifest, signature: signature.toString("hex") };
}
