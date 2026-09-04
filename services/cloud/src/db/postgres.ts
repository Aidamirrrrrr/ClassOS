/**
 * PostgreSQL-адаптер хранилища Cloud v0 (spec T8 §4.1).
 *
 * Тонкий слой поверх `schema.sql`: доменных решений здесь нет, они остаются в
 * `src/domain`. Единственное, что этот файл обязан гарантировать — точное
 * соответствие тем же контрактам, что и `MemoryStore`, иначе тесты на
 * in-memory перестают что-либо значить для реального развёртывания.
 *
 * Приватного ключа устройства нет ни в одном запросе: соответствующей
 * колонки не существует (§4.2, инвариант T8 §12.1-12.2).
 *
 * Время в схеме хранится как `timestamptz`, а в домене — как unix-миллисекунды.
 * Преобразование выполняется здесь и только здесь.
 */

import { SQL } from "bun";

import type { Role } from "../domain/rbac";
import type { EnrollmentToken } from "../domain/enrollment";
import type { AgentVersion, Channel } from "../domain/updates";
import type { AuditEvent } from "../domain/audit";
import type { Session } from "../domain/auth";
import type {
  Branch,
  Device,
  DeviceCertificate,
  Membership,
  Organization,
  Room,
  Store,
  User,
} from "./store";

function toMs(value: Date | string | null): number | null {
  if (value === null) return null;
  return value instanceof Date ? value.getTime() : new Date(value).getTime();
}

function toDate(value: number): Date {
  return new Date(value);
}

function toBuffer(value: Uint8Array | Buffer): Buffer {
  return Buffer.isBuffer(value) ? value : Buffer.from(value);
}

/**
 * Поле `details` хранится в jsonb, но драйвер возвращает его строкой.
 *
 * Разбор выполняется здесь, а не в вызывающем коде: доменный слой работает с
 * `MemoryStore` и обязан получать одинаковый объект от обоих хранилищ, иначе
 * тесты на in-memory перестают что-либо значить для развёртывания.
 */
function parseDetails(value: unknown): Record<string, unknown> {
  if (typeof value === "string") {
    try {
      return JSON.parse(value) as Record<string, unknown>;
    } catch {
      // Нечитаемый аудит — не повод терять само событие.
      return {};
    }
  }
  return (value as Record<string, unknown>) ?? {};
}

export class PostgresStore implements Store {
  private readonly sql: SQL;

  constructor(databaseUrl: string) {
    this.sql = new SQL(databaseUrl);
  }

  /** Применяет схему. Используется развёртыванием и интеграционными тестами. */
  async migrate(schemaSql: string): Promise<void> {
    await this.sql.unsafe(schemaSql);
  }

  async close(): Promise<void> {
    await this.sql.end();
  }

  // --- пользователи и сессии ---------------------------------------------

  async findUserByEmail(email: string): Promise<User | undefined> {
    const rows = await this.sql`
      SELECT id, email, password_hash, display_name FROM users WHERE email = ${email}
    `;
    const row = rows[0];
    return row
      ? {
          id: row.id,
          email: row.email,
          passwordHash: row.password_hash,
          displayName: row.display_name,
        }
      : undefined;
  }

  async findUserById(id: string): Promise<User | undefined> {
    const rows = await this.sql`
      SELECT id, email, password_hash, display_name FROM users WHERE id = ${id}
    `;
    const row = rows[0];
    return row
      ? {
          id: row.id,
          email: row.email,
          passwordHash: row.password_hash,
          displayName: row.display_name,
        }
      : undefined;
  }

  async createUser(user: User): Promise<void> {
    await this.sql`
      INSERT INTO users (id, email, password_hash, display_name)
      VALUES (${user.id}, ${user.email}, ${user.passwordHash}, ${user.displayName})
    `;
  }

  async saveSession(session: Session): Promise<void> {
    // Повторный вход тем же токеном невозможен (токен случайный), но
    // ON CONFLICT оставлен намеренно: молча падать на гонке хуже, чем
    // продлить существующую строку.
    await this.sql`
      INSERT INTO sessions (token_hash, user_id, expires_at)
      VALUES (${session.tokenHash}, ${session.userId}, ${toDate(session.expiresAtUnixMs)})
      ON CONFLICT (token_hash) DO UPDATE SET expires_at = EXCLUDED.expires_at
    `;
  }

  async findSession(tokenHash: Buffer): Promise<Session | undefined> {
    const rows = await this.sql`
      SELECT token_hash, user_id, expires_at FROM sessions WHERE token_hash = ${tokenHash}
    `;
    const row = rows[0];
    return row
      ? {
          tokenHash: toBuffer(row.token_hash),
          userId: row.user_id,
          expiresAtUnixMs: toMs(row.expires_at)!,
        }
      : undefined;
  }

  // --- организации, филиалы, кабинеты ------------------------------------

  async membershipsOf(userId: string): Promise<readonly Membership[]> {
    const rows = await this.sql`
      SELECT organization_id, user_id, branch_id, role
      FROM organization_users WHERE user_id = ${userId}
    `;
    return rows.map((row: Record<string, unknown>) => ({
      organizationId: row.organization_id as string,
      userId: row.user_id as string,
      branchId: (row.branch_id as string | null) ?? null,
      role: row.role as Role,
    }));
  }

  async addMembership(membership: Membership): Promise<void> {
    await this.sql`
      INSERT INTO organization_users (organization_id, user_id, branch_id, role)
      VALUES (${membership.organizationId}, ${membership.userId}, ${membership.branchId}, ${membership.role})
    `;
  }

  async createOrganization(organization: Organization): Promise<void> {
    await this.sql`
      INSERT INTO organizations (id, name) VALUES (${organization.id}, ${organization.name})
    `;
  }

  async createBranch(branch: Branch): Promise<void> {
    await this.sql`
      INSERT INTO branches (id, organization_id, name)
      VALUES (${branch.id}, ${branch.organizationId}, ${branch.name})
    `;
  }

  async branchesOf(organizationId: string): Promise<readonly Branch[]> {
    const rows = await this.sql`
      SELECT id, organization_id, name FROM branches WHERE organization_id = ${organizationId}
    `;
    return rows.map((row: Record<string, unknown>) => ({
      id: row.id as string,
      organizationId: row.organization_id as string,
      name: row.name as string,
    }));
  }

  async findBranch(id: string): Promise<Branch | undefined> {
    const rows = await this.sql`
      SELECT id, organization_id, name FROM branches WHERE id = ${id}
    `;
    const row = rows[0];
    return row ? { id: row.id, organizationId: row.organization_id, name: row.name } : undefined;
  }

  async createRoom(room: Room): Promise<void> {
    await this.sql`
      INSERT INTO rooms (id, branch_id, name, update_channel, desired_profile_id)
      VALUES (${room.id}, ${room.branchId}, ${room.name}, ${room.updateChannel}, ${room.desiredProfileId})
    `;
  }

  async findRoom(id: string): Promise<Room | undefined> {
    const rows = await this.sql`
      SELECT id, branch_id, name, update_channel, desired_profile_id FROM rooms WHERE id = ${id}
    `;
    const row = rows[0];
    return row ? this.roomFrom(row) : undefined;
  }

  async roomsOf(branchId: string): Promise<readonly Room[]> {
    const rows = await this.sql`
      SELECT id, branch_id, name, update_channel, desired_profile_id
      FROM rooms WHERE branch_id = ${branchId} ORDER BY name
    `;
    return rows.map((row: Record<string, unknown>) => this.roomFrom(row));
  }

  private roomFrom(row: Record<string, unknown>): Room {
    return {
      id: row.id as string,
      branchId: row.branch_id as string,
      name: row.name as string,
      updateChannel: row.update_channel as Channel,
      desiredProfileId: (row.desired_profile_id as string | null) ?? null,
    };
  }

  // --- устройства ---------------------------------------------------------

  async upsertDevice(device: Device): Promise<void> {
    await this.sql`
      INSERT INTO devices (id, organization_id, branch_id, room_id, hostname, agent_version, health_state, last_seen_at)
      VALUES (
        ${device.id}, ${device.organizationId}, ${device.branchId}, ${device.roomId},
        ${device.hostname}, ${device.agentVersion}, ${device.healthState},
        ${device.lastSeenAtUnixMs === null ? null : toDate(device.lastSeenAtUnixMs)}
      )
      ON CONFLICT (id) DO UPDATE SET
        branch_id = EXCLUDED.branch_id,
        room_id = EXCLUDED.room_id,
        hostname = EXCLUDED.hostname,
        agent_version = EXCLUDED.agent_version,
        health_state = EXCLUDED.health_state,
        last_seen_at = EXCLUDED.last_seen_at
    `;
  }

  async findDevice(id: string): Promise<Device | undefined> {
    const rows = await this.sql`
      SELECT id, organization_id, branch_id, room_id, hostname, agent_version, health_state, last_seen_at
      FROM devices WHERE id = ${id}
    `;
    const row = rows[0];
    return row ? this.deviceFrom(row) : undefined;
  }

  async devicesOf(branchId: string): Promise<readonly Device[]> {
    const rows = await this.sql`
      SELECT id, organization_id, branch_id, room_id, hostname, agent_version, health_state, last_seen_at
      FROM devices WHERE branch_id = ${branchId} ORDER BY hostname
    `;
    return rows.map((row: Record<string, unknown>) => this.deviceFrom(row));
  }

  private deviceFrom(row: Record<string, unknown>): Device {
    return {
      id: row.id as string,
      organizationId: row.organization_id as string,
      branchId: row.branch_id as string,
      roomId: (row.room_id as string | null) ?? null,
      hostname: row.hostname as string,
      agentVersion: (row.agent_version as string | null) ?? null,
      healthState: (row.health_state as Device["healthState"]) ?? null,
      lastSeenAtUnixMs: toMs(row.last_seen_at as Date | null),
    };
  }

  async saveDeviceCertificate(certificate: DeviceCertificate): Promise<void> {
    await this.sql`
      INSERT INTO device_certificates (device_id, certificate_der, fingerprint_sha256, expires_at)
      VALUES (
        ${certificate.deviceId}, ${toBuffer(certificate.certificateDer)},
        ${toBuffer(certificate.fingerprintSha256)}, ${toDate(certificate.expiresAtUnixMs)}
      )
      ON CONFLICT (device_id) DO UPDATE SET
        certificate_der = EXCLUDED.certificate_der,
        fingerprint_sha256 = EXCLUDED.fingerprint_sha256,
        expires_at = EXCLUDED.expires_at
    `;
  }

  // --- enrollment ---------------------------------------------------------

  async saveEnrollmentToken(token: EnrollmentToken): Promise<void> {
    await this.sql`
      INSERT INTO enrollment_tokens (code_hash, organization_id, branch_id, room_id, created_by, expires_at)
      VALUES (
        ${token.codeHash}, ${token.organizationId}, ${token.branchId}, ${token.roomId},
        ${token.createdBy}, ${toDate(token.expiresAtUnixMs)}
      )
    `;
  }

  async findEnrollmentToken(codeHash: Buffer): Promise<EnrollmentToken | undefined> {
    const rows = await this.sql`
      SELECT code_hash, organization_id, branch_id, room_id, created_by, expires_at, used_at, used_by_device
      FROM enrollment_tokens WHERE code_hash = ${codeHash}
    `;
    const row = rows[0];
    return row
      ? {
          codeHash: toBuffer(row.code_hash),
          organizationId: row.organization_id,
          branchId: row.branch_id,
          roomId: row.room_id ?? null,
          createdBy: row.created_by,
          expiresAtUnixMs: toMs(row.expires_at)!,
          usedAtUnixMs: toMs(row.used_at),
          usedByDevice: row.used_by_device ?? null,
        }
      : undefined;
  }

  async markEnrollmentTokenUsed(
    codeHash: Buffer,
    deviceId: string,
    nowUnixMs: number,
  ): Promise<void> {
    // Строка не удаляется: использованный код должен оставаться в аудите
    // (см. комментарий в schema.sql).
    await this.sql`
      UPDATE enrollment_tokens
      SET used_at = ${toDate(nowUnixMs)}, used_by_device = ${deviceId}
      WHERE code_hash = ${codeHash}
    `;
  }

  // --- обновления ---------------------------------------------------------

  async publishAgentVersion(version: AgentVersion): Promise<void> {
    await this.sql`
      INSERT INTO agent_versions (version, release_channel, url, sha256, signature, minimum_supported_version, published_at)
      VALUES (
        ${version.version}, ${version.channel}, ${version.url}, ${version.sha256},
        ${Buffer.from(version.signatureHex, "hex")}, ${version.minimumSupportedVersion},
        ${toDate(version.publishedAtUnixMs)}
      )
      ON CONFLICT (version, release_channel) DO UPDATE SET
        url = EXCLUDED.url,
        sha256 = EXCLUDED.sha256,
        signature = EXCLUDED.signature,
        minimum_supported_version = EXCLUDED.minimum_supported_version,
        published_at = EXCLUDED.published_at
    `;
  }

  async agentVersions(): Promise<readonly AgentVersion[]> {
    const rows = await this.sql`
      SELECT version, release_channel, url, sha256, signature, minimum_supported_version, published_at
      FROM agent_versions
    `;
    return rows.map((row: Record<string, unknown>) => ({
      version: row.version as string,
      channel: row.release_channel as Channel,
      url: row.url as string,
      sha256: row.sha256 as string,
      signatureHex: toBuffer(row.signature as Uint8Array).toString("hex"),
      minimumSupportedVersion: row.minimum_supported_version as string,
      publishedAtUnixMs: toMs(row.published_at as Date)!,
    }));
  }

  // --- аудит --------------------------------------------------------------

  async appendAudit(event: AuditEvent): Promise<void> {
    await this.sql`
      INSERT INTO audit_events (organization_id, actor_user_id, device_id, action, outcome, details, occurred_at)
      VALUES (
        ${event.organizationId}, ${event.actorUserId}, ${event.deviceId}, ${event.action},
        ${event.outcome}, ${JSON.stringify(event.details)}, ${toDate(event.occurredAtUnixMs)}
      )
    `;
  }

  async auditOf(organizationId: string): Promise<readonly AuditEvent[]> {
    const rows = await this.sql`
      SELECT organization_id, actor_user_id, device_id, action, outcome, details, occurred_at
      FROM audit_events WHERE organization_id = ${organizationId} ORDER BY occurred_at
    `;
    return rows.map((row: Record<string, unknown>) => ({
      organizationId: (row.organization_id as string | null) ?? null,
      actorUserId: (row.actor_user_id as string | null) ?? null,
      deviceId: (row.device_id as string | null) ?? null,
      action: row.action as string,
      outcome: row.outcome as AuditEvent["outcome"],
      details: parseDetails(row.details),
      occurredAtUnixMs: toMs(row.occurred_at as Date)!,
    }));
  }
}
