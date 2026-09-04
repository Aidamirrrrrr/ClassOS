/**
 * Публикация обновлений агента (spec T8 §8).
 *
 * Cloud публикует манифест; проверка подписи и хеша выполняется на
 * устройстве (`crates/updater`). Здесь — только выбор версии для канала.
 */

export const CHANNELS = ["stable", "beta", "canary"] as const;
export type Channel = (typeof CHANNELS)[number];

export interface AgentVersion {
  readonly version: string;
  readonly channel: Channel;
  readonly url: string;
  readonly sha256: string;
  readonly signatureHex: string;
  readonly minimumSupportedVersion: string;
  readonly publishedAtUnixMs: number;
}

function parseVersion(value: string): readonly number[] {
  return value.split(".").map((part) => Number.parseInt(part, 10) || 0);
}

export function compareVersions(left: string, right: string): number {
  const a = parseVersion(left);
  const b = parseVersion(right);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const diff = (a[index] ?? 0) - (b[index] ?? 0);
    if (diff !== 0) return diff > 0 ? 1 : -1;
  }
  return 0;
}

/**
 * Последняя версия канала.
 *
 * Каналы не наследуются: устройство на stable не получает beta-сборку, даже
 * если она новее (spec T8 §8.1).
 */
export function latestForChannel(
  versions: readonly AgentVersion[],
  channel: Channel,
): AgentVersion | undefined {
  return versions
    .filter((value) => value.channel === channel)
    .sort((left, right) => compareVersions(right.version, left.version))[0];
}

/**
 * Манифест в том виде, в каком его разбирает агент (`updater::UpdateManifest`).
 *
 * Имена полей — контракт между Cloud и устройством: расхождение ломает
 * обновление молча, поэтому оно закреплено тестом с обеих сторон (ADR-0015).
 */
export interface UpdateManifest {
  readonly version: string;
  readonly url: string;
  readonly sha256: string;
  readonly signature: string;
  readonly minimum_supported_version: string;
  readonly release_channel: string;
}

export function toManifest(version: AgentVersion): UpdateManifest {
  return {
    version: version.version,
    url: version.url,
    sha256: version.sha256,
    signature: version.signatureHex,
    minimum_supported_version: version.minimumSupportedVersion,
    release_channel: version.channel,
  };
}

/** Есть ли для устройства обновление новее текущей версии. */
export function updateFor(
  versions: readonly AgentVersion[],
  channel: Channel,
  currentVersion: string,
): AgentVersion | undefined {
  const latest = latestForChannel(versions, channel);
  if (!latest) return undefined;
  if (compareVersions(latest.version, currentVersion) <= 0) return undefined;
  if (compareVersions(currentVersion, latest.minimumSupportedVersion) < 0) return undefined;
  return latest;
}
