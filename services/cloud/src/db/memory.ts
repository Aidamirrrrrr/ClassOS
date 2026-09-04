/**
 * In-memory реализация хранилища.
 *
 * Используется в тестах и при локальном запуске без PostgreSQL. Это не
 * "заглушка на будущее", а способ проверять HTTP-слой и авторизацию без
 * поднятой базы.
 */

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
import type { EnrollmentToken } from "../domain/enrollment";
import type { AgentVersion } from "../domain/updates";
import type { AuditEvent } from "../domain/audit";
import type { Session } from "../domain/auth";

export class MemoryStore implements Store {
  private users = new Map<string, User>();
  private sessions = new Map<string, Session>();
  private memberships: Membership[] = [];
  private organizations = new Map<string, Organization>();
  private branches = new Map<string, Branch>();
  private rooms = new Map<string, Room>();
  private devices = new Map<string, Device>();
  private certificates = new Map<string, DeviceCertificate>();
  private enrollmentTokens = new Map<string, EnrollmentToken>();
  private versions: AgentVersion[] = [];
  private audit: AuditEvent[] = [];

  async findUserByEmail(email: string): Promise<User | undefined> {
    return [...this.users.values()].find(
      (user) => user.email.toLowerCase() === email.toLowerCase(),
    );
  }
  async findUserById(id: string): Promise<User | undefined> {
    return this.users.get(id);
  }
  async createUser(user: User): Promise<void> {
    this.users.set(user.id, user);
  }

  async saveSession(session: Session): Promise<void> {
    this.sessions.set(session.tokenHash.toString("hex"), session);
  }
  async findSession(tokenHash: Buffer): Promise<Session | undefined> {
    return this.sessions.get(tokenHash.toString("hex"));
  }

  async membershipsOf(userId: string): Promise<readonly Membership[]> {
    return this.memberships.filter((value) => value.userId === userId);
  }
  async addMembership(membership: Membership): Promise<void> {
    this.memberships.push(membership);
  }

  async createOrganization(organization: Organization): Promise<void> {
    this.organizations.set(organization.id, organization);
  }
  async createBranch(branch: Branch): Promise<void> {
    this.branches.set(branch.id, branch);
  }
  async branchesOf(organizationId: string): Promise<readonly Branch[]> {
    return [...this.branches.values()].filter((value) => value.organizationId === organizationId);
  }
  async findBranch(id: string): Promise<Branch | undefined> {
    return this.branches.get(id);
  }

  async createRoom(room: Room): Promise<void> {
    this.rooms.set(room.id, room);
  }
  async findRoom(id: string): Promise<Room | undefined> {
    return this.rooms.get(id);
  }
  async roomsOf(branchId: string): Promise<readonly Room[]> {
    return [...this.rooms.values()].filter((value) => value.branchId === branchId);
  }

  async upsertDevice(device: Device): Promise<void> {
    this.devices.set(device.id, device);
  }
  async findDevice(id: string): Promise<Device | undefined> {
    return this.devices.get(id);
  }
  async devicesOf(branchId: string): Promise<readonly Device[]> {
    return [...this.devices.values()].filter((value) => value.branchId === branchId);
  }
  async saveDeviceCertificate(certificate: DeviceCertificate): Promise<void> {
    this.certificates.set(certificate.deviceId, certificate);
  }

  async saveEnrollmentToken(token: EnrollmentToken): Promise<void> {
    this.enrollmentTokens.set(token.codeHash.toString("hex"), token);
  }
  async findEnrollmentToken(codeHash: Buffer): Promise<EnrollmentToken | undefined> {
    return this.enrollmentTokens.get(codeHash.toString("hex"));
  }
  async markEnrollmentTokenUsed(
    codeHash: Buffer,
    deviceId: string,
    nowUnixMs: number,
  ): Promise<void> {
    const token = this.enrollmentTokens.get(codeHash.toString("hex"));
    if (!token) return;
    token.usedAtUnixMs = nowUnixMs;
    token.usedByDevice = deviceId;
  }

  async publishAgentVersion(version: AgentVersion): Promise<void> {
    this.versions = this.versions.filter(
      (value) => !(value.version === version.version && value.channel === version.channel),
    );
    this.versions.push(version);
  }
  async agentVersions(): Promise<readonly AgentVersion[]> {
    return this.versions;
  }

  async appendAudit(event: AuditEvent): Promise<void> {
    this.audit.push(event);
  }
  async auditOf(organizationId: string): Promise<readonly AuditEvent[]> {
    return this.audit.filter((value) => value.organizationId === organizationId);
  }
}
