import { describe, expect, test } from "bun:test";
import { authorize, can, permissionsFor } from "../src/domain/rbac";

describe("матрица прав", () => {
  test("teacher не управляет организацией, филиалом и биллингом", () => {
    for (const permission of ["organization:manage", "billing:manage", "branch:manage", "teacher:manage", "update:publish"] as const) {
      expect(can("teacher", permission)).toBe(false);
    }
  });

  test("teacher ведёт урок", () => {
    expect(can("teacher", "classroom:view")).toBe(true);
    expect(can("teacher", "classroom:control")).toBe(true);
    expect(can("teacher", "lesson_profile:apply")).toBe(true);
  });

  test("admin управляет филиалом, но не организацией и не биллингом", () => {
    expect(can("admin", "branch:manage")).toBe(true);
    expect(can("admin", "organization:manage")).toBe(false);
    expect(can("admin", "billing:manage")).toBe(false);
  });

  test("owner имеет все права", () => {
    expect(permissionsFor("owner").length).toBeGreaterThan(permissionsFor("admin").length);
    expect(can("owner", "billing:manage")).toBe(true);
  });
});

describe("область действия членства", () => {
  const branchAdmin = { organizationId: "org-1", branchId: "branch-1", role: "admin" } as const;

  test("admin филиала не управляет другим филиалом", () => {
    expect(authorize(branchAdmin, "room:manage", { organizationId: "org-1", branchId: "branch-1" })).toBe(true);
    expect(authorize(branchAdmin, "room:manage", { organizationId: "org-1", branchId: "branch-2" })).toBe(false);
  });

  test("членство не действует в чужой организации", () => {
    expect(authorize(branchAdmin, "room:manage", { organizationId: "org-2", branchId: "branch-1" })).toBe(false);
  });

  test("членство уровня организации действует в любом её филиале", () => {
    const owner = { organizationId: "org-1", branchId: null, role: "owner" } as const;
    expect(authorize(owner, "room:manage", { organizationId: "org-1", branchId: "branch-9" })).toBe(true);
  });
});
