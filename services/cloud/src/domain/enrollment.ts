/**
 * Одноразовые enrollment-коды (spec T8 §6).
 *
 * Формат протокольных сообщений EnrollmentRequest/EnrollmentResult при
 * переезде с локальной заглушки T1 на Cloud не меняется — меняется только то,
 * кто выпускает код и сертификат (ADR-0007).
 */

import { createHash, randomBytes, timingSafeEqual } from "node:crypto";

/** Алфавит без символов, которые путают при диктовке по телефону. */
const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH = 8;
export const DEFAULT_TTL_MS = 15 * 60 * 1000;

export interface EnrollmentToken {
  readonly codeHash: Buffer;
  readonly organizationId: string;
  readonly branchId: string;
  readonly roomId: string | null;
  readonly createdBy: string;
  readonly expiresAtUnixMs: number;
  usedAtUnixMs: number | null;
  usedByDevice: string | null;
}

/**
 * Код хранится только в виде хеша: утечка дампа БД не должна давать
 * возможности зарегистрировать устройство.
 */
export function hashCode(code: string): Buffer {
  return createHash("sha256").update(code.trim().toUpperCase()).digest();
}

export function generateCode(): string {
  const bytes = randomBytes(CODE_LENGTH);
  let code = "";
  for (let index = 0; index < CODE_LENGTH; index += 1) {
    code += ALPHABET[bytes[index]! % ALPHABET.length];
  }
  return code;
}

export type EnrollmentFailure =
  | "ENROLLMENT_ERROR_CODE_INVALID"
  | "ENROLLMENT_ERROR_CODE_EXPIRED"
  | "ENROLLMENT_ERROR_CODE_ALREADY_USED";

export type EnrollmentCheck =
  | { readonly ok: true; readonly token: EnrollmentToken }
  | { readonly ok: false; readonly error: EnrollmentFailure };

/**
 * Проверяет код против выданного токена.
 *
 * Сравнение хешей постоянное по времени: код короткий, и утечка через тайминг
 * заметно сокращает перебор.
 */
export function checkCode(
  code: string,
  token: EnrollmentToken | undefined,
  nowUnixMs: number,
): EnrollmentCheck {
  if (!token) return { ok: false, error: "ENROLLMENT_ERROR_CODE_INVALID" };
  const provided = hashCode(code);
  if (provided.length !== token.codeHash.length || !timingSafeEqual(provided, token.codeHash)) {
    return { ok: false, error: "ENROLLMENT_ERROR_CODE_INVALID" };
  }
  // Порядок проверок важен: "использован" сообщается раньше "истёк", потому
  // что использованный код не должен выглядеть как просто просроченный.
  if (token.usedAtUnixMs !== null) return { ok: false, error: "ENROLLMENT_ERROR_CODE_ALREADY_USED" };
  if (nowUnixMs >= token.expiresAtUnixMs) return { ok: false, error: "ENROLLMENT_ERROR_CODE_EXPIRED" };
  return { ok: true, token };
}

export function issueToken(input: {
  organizationId: string;
  branchId: string;
  roomId?: string | null;
  createdBy: string;
  nowUnixMs: number;
  ttlMs?: number;
}): { code: string; token: EnrollmentToken } {
  const code = generateCode();
  return {
    code,
    token: {
      codeHash: hashCode(code),
      organizationId: input.organizationId,
      branchId: input.branchId,
      roomId: input.roomId ?? null,
      createdBy: input.createdBy,
      expiresAtUnixMs: input.nowUnixMs + (input.ttlMs ?? DEFAULT_TTL_MS),
      usedAtUnixMs: null,
      usedByDevice: null,
    },
  };
}
