/**
 * Кросс-языковой контракт подписи манифеста (spec T8 §8.2, ADR-0015).
 *
 * Те же байты и та же подпись проверяются тестом
 * `manifest_signed_by_cloud_is_accepted` в `crates/updater/src/lib.rs`.
 * Расхождение форматов означало бы, что ни одно обновление не установится —
 * и обнаружилось бы это только на реальном устройстве.
 */

import { describe, expect, test } from "bun:test";

import { manifestPayload, signManifest } from "../src/domain/manifest";

const SEED = new Uint8Array(32).fill(5);

const MANIFEST = {
  version: "0.3.0",
  url: "https://updates.example.org/classos-0.3.0.bin",
  sha256: "0f".repeat(32),
  minimum_supported_version: "0.1.0",
  release_channel: "stable" as const,
};

describe("подпись манифеста", () => {
  test("поля кодируются с префиксом длины", () => {
    const payload = manifestPayload(MANIFEST);
    // Первое поле — version длиной 5 байт.
    expect(Array.from(payload.slice(0, 4))).toEqual([0, 0, 0, 5]);
    expect(new TextDecoder().decode(payload.slice(4, 9))).toBe("0.3.0");
  });

  test("подпись совпадает с вектором, который проверяет Rust", () => {
    const signed = signManifest(SEED, MANIFEST);
    expect(signed.signature).toBe(
      "c7a1d1140cc36b3724c4e0826d64ff0acaffeffc37accf91245f8c1e7d597b31" +
        "541f036adecfeab3351555476884f3f2efa873c6d11d9e6787a1dda67fb53c0d",
    );
  });
});
