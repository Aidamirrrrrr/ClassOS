/**
 * Журнал значимых действий (spec T8 §4.1, инвариант 6 `CLAUDE.md`).
 *
 * Аудит пишется и для отказов: попытка без прав — самое интересное событие
 * в этом журнале.
 */

export type AuditOutcome = "success" | "failure";

export interface AuditEvent {
  readonly organizationId: string | null;
  readonly actorUserId: string | null;
  readonly deviceId: string | null;
  readonly action: string;
  readonly outcome: AuditOutcome;
  readonly details: Record<string, unknown>;
  readonly occurredAtUnixMs: number;
}

export function auditEvent(input: {
  organizationId?: string | null;
  actorUserId?: string | null;
  deviceId?: string | null;
  action: string;
  outcome: AuditOutcome;
  details?: Record<string, unknown>;
  nowUnixMs: number;
}): AuditEvent {
  return {
    organizationId: input.organizationId ?? null,
    actorUserId: input.actorUserId ?? null,
    deviceId: input.deviceId ?? null,
    action: input.action,
    outcome: input.outcome,
    details: input.details ?? {},
    occurredAtUnixMs: input.nowUnixMs,
  };
}
