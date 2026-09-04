/**
 * HTTP API Cloud v0.
 *
 * Один modular monolith, не микросервисы (`01_TECHNICAL_ARCHITECTURE.md` §98).
 * Каждый маршрут, меняющий состояние, проходит через `requirePermission`:
 * авторизация не должна зависеть от того, вспомнил ли автор маршрута о ней.
 */

import { randomUUID } from "node:crypto";

import { bearerToken, createSession, hashToken, sessionIsValid, verifyPassword } from "../domain/auth";
import { auditEvent } from "../domain/audit";
import { checkCode, hashCode, issueToken } from "../domain/enrollment";
import { issueLease, type LeasePermission } from "../domain/lease";
import { authorize, type Permission } from "../domain/rbac";
import { updateFor, type Channel } from "../domain/updates";
import type { Membership, Store } from "../db/store";

export interface AppDeps {
  readonly store: Store;
  /** Seed ключа подписи lease. В продакшене приходит из секрет-хранилища. */
  readonly leaseIssuerSeed: Uint8Array;
  readonly now: () => number;
}

interface Actor {
  readonly userId: string;
  readonly memberships: readonly Membership[];
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function error(code: string, status: number): Response {
  return json({ error: code }, status);
}

export function createApp(deps: AppDeps) {
  const { store, now } = deps;

  async function authenticate(request: Request): Promise<Actor | null> {
    const token = bearerToken(request.headers.get("authorization"));
    if (!token) return null;
    const session = await store.findSession(hashToken(token));
    if (!sessionIsValid(session, token, now())) return null;
    return { userId: session!.userId, memberships: await store.membershipsOf(session!.userId) };
  }

  /**
   * Проверка права в конкретной области.
   *
   * Отказ пишется в аудит: попытка выйти за пределы своей роли — событие,
   * которое обязано быть видно.
   */
  async function requirePermission(
    actor: Actor,
    permission: Permission,
    scope: { organizationId: string; branchId?: string },
    action: string,
  ): Promise<boolean> {
    const allowed = actor.memberships.some((membership) =>
      authorize(membership, permission, scope),
    );
    if (!allowed) {
      await store.appendAudit(
        auditEvent({
          organizationId: scope.organizationId,
          actorUserId: actor.userId,
          action,
          outcome: "failure",
          details: { permission, reason: "forbidden" },
          nowUnixMs: now(),
        }),
      );
    }
    return allowed;
  }

  async function handle(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname;
    const method = request.method;

    if (method === "GET" && path === "/health") {
      return json({ status: "ok" });
    }

    // --- аутентификация ---------------------------------------------------
    if (method === "POST" && path === "/v1/auth/login") {
      const body = (await request.json()) as { email?: string; password?: string };
      const user = body.email ? await store.findUserByEmail(body.email) : undefined;
      // Одинаковый ответ на неизвестного пользователя и неверный пароль:
      // иначе API становится способом проверять, кто зарегистрирован.
      const ok = user ? await verifyPassword(body.password ?? "", user.passwordHash) : false;
      if (!user || !ok) return error("INVALID_CREDENTIALS", 401);

      const { token, session } = createSession(user.id, now());
      await store.saveSession(session);
      await store.appendAudit(
        auditEvent({ actorUserId: user.id, action: "auth.login", outcome: "success", nowUnixMs: now() }),
      );
      return json({ token, expires_at_unix_ms: session.expiresAtUnixMs });
    }

    const actor = await authenticate(request);
    if (!actor) return error("UNAUTHORIZED", 401);

    if (method === "GET" && path === "/v1/me") {
      return json({ user_id: actor.userId, memberships: actor.memberships });
    }

    // --- филиалы и кабинеты ----------------------------------------------
    if (method === "POST" && path === "/v1/branches") {
      const body = (await request.json()) as { organization_id?: string; name?: string };
      if (!body.organization_id || !body.name) return error("INVALID_REQUEST", 400);
      if (!(await requirePermission(actor, "branch:manage", { organizationId: body.organization_id }, "branch.create"))) {
        return error("FORBIDDEN", 403);
      }
      const branch = { id: randomUUID(), organizationId: body.organization_id, name: body.name };
      await store.createBranch(branch);
      return json(branch, 201);
    }

    if (method === "POST" && path === "/v1/rooms") {
      const body = (await request.json()) as {
        branch_id?: string;
        name?: string;
        update_channel?: Channel;
        desired_profile_id?: string;
      };
      if (!body.branch_id || !body.name) return error("INVALID_REQUEST", 400);
      const branch = await store.findBranch(body.branch_id);
      if (!branch) return error("BRANCH_NOT_FOUND", 404);
      if (!(await requirePermission(actor, "room:manage", { organizationId: branch.organizationId, branchId: branch.id }, "room.create"))) {
        return error("FORBIDDEN", 403);
      }
      const room = {
        id: randomUUID(),
        branchId: branch.id,
        name: body.name,
        updateChannel: body.update_channel ?? ("stable" as Channel),
        desiredProfileId: body.desired_profile_id ?? null,
      };
      await store.createRoom(room);
      return json(room, 201);
    }

    if (method === "GET" && path.startsWith("/v1/branches/") && path.endsWith("/devices")) {
      const branchId = path.split("/")[3]!;
      const branch = await store.findBranch(branchId);
      if (!branch) return error("BRANCH_NOT_FOUND", 404);
      if (!(await requirePermission(actor, "health:view", { organizationId: branch.organizationId, branchId }, "device.list"))) {
        return error("FORBIDDEN", 403);
      }
      return json({ devices: await store.devicesOf(branchId) });
    }

    // --- enrollment -------------------------------------------------------
    if (method === "POST" && path === "/v1/enrollment/codes") {
      const body = (await request.json()) as { branch_id?: string; room_id?: string };
      if (!body.branch_id) return error("INVALID_REQUEST", 400);
      const branch = await store.findBranch(body.branch_id);
      if (!branch) return error("BRANCH_NOT_FOUND", 404);
      if (!(await requirePermission(actor, "device:enroll", { organizationId: branch.organizationId, branchId: branch.id }, "enrollment.issue"))) {
        return error("FORBIDDEN", 403);
      }
      const { code, token } = issueToken({
        organizationId: branch.organizationId,
        branchId: branch.id,
        roomId: body.room_id ?? null,
        createdBy: actor.userId,
        nowUnixMs: now(),
      });
      await store.saveEnrollmentToken(token);
      await store.appendAudit(
        auditEvent({
          organizationId: branch.organizationId,
          actorUserId: actor.userId,
          action: "enrollment.issue",
          outcome: "success",
          nowUnixMs: now(),
        }),
      );
      return json({ code, expires_at_unix_ms: token.expiresAtUnixMs }, 201);
    }

    // --- classroom lease --------------------------------------------------
    if (method === "POST" && path === "/v1/lease") {
      const body = (await request.json()) as { organization_id?: string; branch_id?: string };
      if (!body.organization_id || !body.branch_id) return error("INVALID_REQUEST", 400);
      if (!(await requirePermission(actor, "classroom:view", { organizationId: body.organization_id, branchId: body.branch_id }, "lease.issue"))) {
        return error("FORBIDDEN", 403);
      }
      const rooms = await store.roomsOf(body.branch_id);
      // Lease перечисляет ровно те права, что есть у роли: расширить его
      // запросом нельзя (spec T8 §12.5).
      const membership = actor.memberships.find((value) =>
        authorize(value, "classroom:view", {
          organizationId: body.organization_id!,
          branchId: body.branch_id,
        }),
      )!;
      const permissions: LeasePermission[] = ["view_classroom"];
      if (authorize(membership, "classroom:control", { organizationId: body.organization_id, branchId: body.branch_id })) {
        permissions.push("control_classroom");
      }
      if (authorize(membership, "lesson_profile:apply", { organizationId: body.organization_id, branchId: body.branch_id })) {
        permissions.push("apply_lesson_profile");
      }
      if (authorize(membership, "device:repair", { organizationId: body.organization_id, branchId: body.branch_id })) {
        permissions.push("repair_devices");
      }

      const issuedAt = now();
      const signed = issueLease(deps.leaseIssuerSeed, {
        teacherId: actor.userId,
        organizationId: body.organization_id,
        branchId: body.branch_id,
        allowedRooms: rooms.map((room) => room.id),
        permissions,
        issuedAtUnixMs: issuedAt,
        // 12 часов: урок переживает потерю интернета, но lease не становится
        // бессрочным пропуском (spec T8 §7).
        expiresAtUnixMs: issuedAt + 12 * 60 * 60 * 1000,
      });
      return json(signed);
    }

    // --- обновления -------------------------------------------------------
    if (method === "POST" && path === "/v1/updates") {
      const body = (await request.json()) as {
        organization_id?: string;
        version?: string;
        channel?: Channel;
        url?: string;
        sha256?: string;
        signature_hex?: string;
        minimum_supported_version?: string;
      };
      if (!body.organization_id || !body.version || !body.channel || !body.url || !body.sha256) {
        return error("INVALID_REQUEST", 400);
      }
      if (!(await requirePermission(actor, "update:publish", { organizationId: body.organization_id }, "update.publish"))) {
        return error("FORBIDDEN", 403);
      }
      await store.publishAgentVersion({
        version: body.version,
        channel: body.channel,
        url: body.url,
        sha256: body.sha256,
        signatureHex: body.signature_hex ?? "",
        minimumSupportedVersion: body.minimum_supported_version ?? "0.1.0",
        publishedAtUnixMs: now(),
      });
      return json({ published: true }, 201);
    }

    if (method === "GET" && path === "/v1/updates/check") {
      const channel = (url.searchParams.get("channel") ?? "stable") as Channel;
      const current = url.searchParams.get("current_version") ?? "0.0.0";
      const update = updateFor(await store.agentVersions(), channel, current);
      return json({ update: update ?? null });
    }

    // --- аудит ------------------------------------------------------------
    if (method === "GET" && path === "/v1/audit") {
      const organizationId = url.searchParams.get("organization_id");
      if (!organizationId) return error("INVALID_REQUEST", 400);
      if (!(await requirePermission(actor, "audit:view", { organizationId }, "audit.read"))) {
        return error("FORBIDDEN", 403);
      }
      return json({ events: await store.auditOf(organizationId) });
    }

    return error("NOT_FOUND", 404);
  }

  return { fetch: (request: Request) => handle(request) };
}

/** Регистрация устройства по одноразовому коду; вызывается установщиком. */
export function enrollmentHandler(deps: AppDeps) {
  const { store, now } = deps;
  return async function enroll(request: Request): Promise<Response> {
    const body = (await request.json()) as {
      code?: string;
      hostname?: string;
      certificate_der_base64?: string;
    };
    if (!body.code || !body.hostname || !body.certificate_der_base64) {
      return error("ENROLLMENT_ERROR_CODE_INVALID", 400);
    }
    const token = await store.findEnrollmentToken(hashCode(body.code));
    const check = checkCode(body.code, token, now());
    if (!check.ok) {
      await store.appendAudit(
        auditEvent({
          organizationId: token?.organizationId ?? null,
          action: "device.enroll",
          outcome: "failure",
          details: { error: check.error },
          nowUnixMs: now(),
        }),
      );
      return error(check.error, 400);
    }

    const certificate = Buffer.from(body.certificate_der_base64, "base64");
    const deviceId = randomUUID();
    await store.upsertDevice({
      id: deviceId,
      organizationId: check.token.organizationId,
      branchId: check.token.branchId,
      roomId: check.token.roomId,
      hostname: body.hostname,
      agentVersion: null,
      healthState: null,
      lastSeenAtUnixMs: now(),
    });
    // Сохраняется только публичный сертификат: приватный ключ остаётся на
    // устройстве и в Cloud не передаётся (инвариант T8 §12.1).
    await store.saveDeviceCertificate({
      deviceId,
      certificateDer: new Uint8Array(certificate),
      fingerprintSha256: new Uint8Array(
        await crypto.subtle.digest("SHA-256", certificate),
      ),
      expiresAtUnixMs: now() + 365 * 24 * 60 * 60 * 1000,
    });
    await store.markEnrollmentTokenUsed(check.token.codeHash, deviceId, now());
    await store.appendAudit(
      auditEvent({
        organizationId: check.token.organizationId,
        deviceId,
        action: "device.enroll",
        outcome: "success",
        nowUnixMs: now(),
      }),
    );
    return json({ device_id: deviceId, branch_id: check.token.branchId }, 201);
  };
}
