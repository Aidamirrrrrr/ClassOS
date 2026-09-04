/**
 * Аутентификация пользователей Cloud.
 *
 * Пароли хешируются argon2id средствами Bun; в БД попадает только хеш
 * (чеклист T8 §10, пункт "no plaintext secrets").
 */

import { randomBytes, createHash, timingSafeEqual } from "node:crypto";

export const SESSION_TTL_MS = 12 * 60 * 60 * 1000;

export async function hashPassword(password: string): Promise<string> {
  return Bun.password.hash(password, { algorithm: "argon2id" });
}

export async function verifyPassword(password: string, hash: string): Promise<boolean> {
  try {
    return await Bun.password.verify(password, hash);
  } catch {
    // Повреждённый хеш — это отказ в аутентификации, а не 500.
    return false;
  }
}

export interface Session {
  readonly tokenHash: Buffer;
  readonly userId: string;
  readonly expiresAtUnixMs: number;
}

/** Токен сессии хранится хешем: дамп таблицы не должен давать вход в систему. */
export function hashToken(token: string): Buffer {
  return createHash("sha256").update(token).digest();
}

export function createSession(userId: string, nowUnixMs: number): { token: string; session: Session } {
  const token = randomBytes(32).toString("base64url");
  return {
    token,
    session: { tokenHash: hashToken(token), userId, expiresAtUnixMs: nowUnixMs + SESSION_TTL_MS },
  };
}

export function sessionIsValid(
  session: Session | undefined,
  token: string,
  nowUnixMs: number,
): boolean {
  if (!session) return false;
  const provided = hashToken(token);
  if (provided.length !== session.tokenHash.length) return false;
  if (!timingSafeEqual(provided, session.tokenHash)) return false;
  return nowUnixMs < session.expiresAtUnixMs;
}

/** Достаёт токен из заголовка Authorization. */
export function bearerToken(header: string | null): string | null {
  if (!header) return null;
  const [scheme, value] = header.split(" ");
  if (scheme?.toLowerCase() !== "bearer" || !value) return null;
  return value;
}
