/**
 * Кросс-языковой контракт манифеста обновления (ADR-0015).
 *
 * Тот же JSON разбирается тестом `manifest_from_cloud_parses` в
 * `crates/agent-service/src/update_checker.rs`. Если одна сторона переименует
 * поле, разойдутся оба теста, а не поведение на реальном устройстве.
 */

import { describe, expect, test } from "bun:test";
import { toManifest, updateFor, type AgentVersion } from "../src/domain/updates";

const PUBLISHED: AgentVersion = {
  version: "0.3.0",
  channel: "stable",
  url: "https://updates.example.org/classos-0.3.0.bin",
  sha256: "0f".repeat(32),
  signatureHex: "ab".repeat(64),
  minimumSupportedVersion: "0.1.0",
  publishedAtUnixMs: 1_800_000_000_000,
};

/** Ровно этот текст обязан разбирать агент. */
export const MANIFEST_VECTOR = JSON.stringify(toManifest(PUBLISHED));

describe("update manifest", () => {
  test("поля названы так, как их ждёт агент", () => {
    expect(toManifest(PUBLISHED)).toEqual({
      version: "0.3.0",
      url: "https://updates.example.org/classos-0.3.0.bin",
      sha256: "0f".repeat(32),
      signature: "ab".repeat(64),
      minimum_supported_version: "0.1.0",
      release_channel: "stable",
    });
  });

  test("тестовый вектор для Rust-стороны не менялся", () => {
    expect(MANIFEST_VECTOR).toBe(
      '{"version":"0.3.0","url":"https://updates.example.org/classos-0.3.0.bin",' +
        '"sha256":"' + "0f".repeat(32) + '","signature":"' + "ab".repeat(64) + '",' +
        '"minimum_supported_version":"0.1.0","release_channel":"stable"}',
    );
  });

  /** Канал не наследуется: stable-устройство не забирает beta-сборку. */
  test("канал устройства не подменяется более новой сборкой другого канала", () => {
    const beta: AgentVersion = { ...PUBLISHED, version: "0.4.0", channel: "beta" };
    expect(updateFor([PUBLISHED, beta], "stable", "0.2.0")?.version).toBe("0.3.0");
  });
});
