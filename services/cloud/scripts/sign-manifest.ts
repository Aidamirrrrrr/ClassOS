#!/usr/bin/env bun
/**
 * Подписывает манифест обновления агента (spec T8 §8.2).
 *
 * Запускается в среде выпуска релиза, где есть приватный ключ издателя. Ни
 * Cloud в рантайме, ни устройство этого ключа не видят: устройство проверяет
 * подпись публичным ключом, вшитым в `classos-updater.exe` при сборке.
 *
 * Использование:
 *   CLASSOS_PUBLISHER_SEED_HEX=<64 hex> bun scripts/sign-manifest.ts \
 *     --file dist/classos-0.3.0.zip \
 *     --version 0.3.0 \
 *     --url https://updates.example.org/classos-0.3.0.zip \
 *     --channel stable \
 *     --minimum-version 0.1.0
 *
 * Печатает готовый манифест в stdout — ровно тот JSON, который разбирает
 * агент и публикует `POST /v1/updates`.
 */

import { createHash } from "node:crypto";

import { signManifest } from "../src/domain/manifest";
import { CHANNELS, type Channel } from "../src/domain/updates";

function argument(name: string): string | undefined {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

function required(name: string): string {
  const value = argument(name);
  if (!value) {
    console.error(`не задан обязательный аргумент --${name}`);
    process.exit(2);
  }
  return value;
}

const seedHex = process.env.CLASSOS_PUBLISHER_SEED_HEX;
if (!seedHex || !/^[0-9a-fA-F]{64}$/.test(seedHex)) {
  console.error("CLASSOS_PUBLISHER_SEED_HEX должен содержать 32 байта в hex");
  process.exit(2);
}

const channel = required("channel") as Channel;
if (!CHANNELS.includes(channel)) {
  console.error(`неизвестный канал ${channel}; допустимы: ${CHANNELS.join(", ")}`);
  process.exit(2);
}

const filePath = required("file");
const payload = await Bun.file(filePath).arrayBuffer();
const sha256 = createHash("sha256").update(Buffer.from(payload)).digest("hex");

const manifest = signManifest(Uint8Array.from(Buffer.from(seedHex, "hex")), {
  version: required("version"),
  url: required("url"),
  sha256,
  minimum_supported_version: argument("minimum-version") ?? "0.1.0",
  release_channel: channel,
});

console.log(JSON.stringify(manifest, null, 2));
