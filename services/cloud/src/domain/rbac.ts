/**
 * Роли и права Cloud v0 (spec T8 §5).
 *
 * Матрица объявлена данными, а не набором `if` в обработчиках: проверка прав,
 * размазанная по маршрутам, рано или поздно разъезжается с документацией.
 */

export const ROLES = ["owner", "admin", "teacher"] as const;
export type Role = (typeof ROLES)[number];

export const PERMISSIONS = [
  // организация и биллинг
  "organization:manage",
  "billing:manage",
  // филиал, кабинеты, устройства
  "branch:manage",
  "room:manage",
  "device:manage",
  "device:enroll",
  // преподаватели
  "teacher:manage",
  // класс
  "classroom:view",
  "classroom:control",
  "lesson_profile:apply",
  // обслуживание
  "device:repair",
  "health:view",
  "update:publish",
  "audit:view",
] as const;
export type Permission = (typeof PERMISSIONS)[number];

/**
 * Права каждой роли.
 *
 * Teacher намеренно не имеет ни одного `*:manage`: он не меняет политики
 * организации, не управляет биллингом и не ставит привилегированное ПО
 * (spec T8 §5, §12.5).
 */
const MATRIX: Record<Role, readonly Permission[]> = {
  owner: [
    "organization:manage",
    "billing:manage",
    "branch:manage",
    "room:manage",
    "device:manage",
    "device:enroll",
    "teacher:manage",
    "classroom:view",
    "classroom:control",
    "lesson_profile:apply",
    "device:repair",
    "health:view",
    "update:publish",
    "audit:view",
  ],
  admin: [
    "branch:manage",
    "room:manage",
    "device:manage",
    "device:enroll",
    "teacher:manage",
    "classroom:view",
    "classroom:control",
    "lesson_profile:apply",
    "device:repair",
    "health:view",
    "audit:view",
  ],
  teacher: [
    "classroom:view",
    "classroom:control",
    "lesson_profile:apply",
  ],
};

export function permissionsFor(role: Role): readonly Permission[] {
  return MATRIX[role];
}

export function can(role: Role, permission: Permission): boolean {
  return MATRIX[role].includes(permission);
}

/**
 * Членство пользователя в организации с областью действия.
 *
 * `branchId === null` означает права на всю организацию; иначе роль
 * действует только внутри конкретного филиала.
 */
export interface Membership {
  readonly organizationId: string;
  readonly branchId: string | null;
  readonly role: Role;
}

/**
 * Проверяет право с учётом области.
 *
 * Admin одного филиала не получает доступ к другому: ограничение области —
 * такая же часть авторизации, как и сама роль.
 */
export function authorize(
  membership: Membership,
  permission: Permission,
  scope: { organizationId: string; branchId?: string },
): boolean {
  if (membership.organizationId !== scope.organizationId) return false;
  if (membership.branchId !== null && scope.branchId !== membership.branchId) return false;
  return can(membership.role, permission);
}
