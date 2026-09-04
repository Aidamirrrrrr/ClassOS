/**
 * Хранилище Cloud v0.
 *
 * Интерфейс отделён от PostgreSQL намеренно: доменная логика и HTTP-слой
 * тестируются на in-memory реализации, а SQL остаётся тонким адаптером.
 * Схема — `src/db/schema.sql`.
 */

import type { Role } from "../domain/rbac";
import type { EnrollmentToken } from "../domain/enrollment";
import type { AgentVersion, Channel } from "../domain/updates";
import type { AuditEvent } from "../domain/audit";
import type { Session } from "../domain/auth";

export interface User {
  readonly id: string;
  readonly email: string;
  readonly passwordHash: string;
  readonly displayName: string;
}

export interface Organization {
  readonly id: string;
  readonly name: string;
}

export interface Branch {
  readonly id: string;
  readonly organizationId: string;
  readonly name: string;
}

export interface Room {
  readonly id: string;
  readonly branchId: string;
  readonly name: string;
  readonly updateChannel: Channel;
  readonly desiredProfileId: string | null;
}

export interface Device {
  readonly id: string;
  readonly organizationId: string;
  readonly branchId: string;
  readonly roomId: string | null;
  readonly hostname: string;
  readonly agentVersion: string | null;
  readonly healthState: "healthy" | "warning" | "critical" | null;
  readonly lastSeenAtUnixMs: number | null;
}

/**
 * Публичная часть identity устройства.
 *
 * Приватного ключа здесь нет и быть не может: он никогда не покидает
 * устройство (инвариант T8 §12.1-12.2).
 */
export interface DeviceCertificate {
  readonly deviceId: string;
  readonly certificateDer: Uint8Array;
  readonly fingerprintSha256: Uint8Array;
  readonly expiresAtUnixMs: number;
}

export interface Membership {
  readonly organizationId: string;
  readonly userId: string;
  readonly branchId: string | null;
  readonly role: Role;
}

export interface Store {
  findUserByEmail(email: string): Promise<User | undefined>;
  findUserById(id: string): Promise<User | undefined>;
  createUser(user: User): Promise<void>;

  saveSession(session: Session): Promise<void>;
  findSession(tokenHash: Buffer): Promise<Session | undefined>;

  membershipsOf(userId: string): Promise<readonly Membership[]>;
  addMembership(membership: Membership): Promise<void>;

  createOrganization(organization: Organization): Promise<void>;
  createBranch(branch: Branch): Promise<void>;
  branchesOf(organizationId: string): Promise<readonly Branch[]>;
  findBranch(id: string): Promise<Branch | undefined>;

  createRoom(room: Room): Promise<void>;
  findRoom(id: string): Promise<Room | undefined>;
  roomsOf(branchId: string): Promise<readonly Room[]>;

  upsertDevice(device: Device): Promise<void>;
  findDevice(id: string): Promise<Device | undefined>;
  devicesOf(branchId: string): Promise<readonly Device[]>;
  saveDeviceCertificate(certificate: DeviceCertificate): Promise<void>;

  saveEnrollmentToken(token: EnrollmentToken): Promise<void>;
  findEnrollmentToken(codeHash: Buffer): Promise<EnrollmentToken | undefined>;
  markEnrollmentTokenUsed(codeHash: Buffer, deviceId: string, nowUnixMs: number): Promise<void>;

  publishAgentVersion(version: AgentVersion): Promise<void>;
  agentVersions(): Promise<readonly AgentVersion[]>;

  appendAudit(event: AuditEvent): Promise<void>;
  auditOf(organizationId: string): Promise<readonly AuditEvent[]>;
}
