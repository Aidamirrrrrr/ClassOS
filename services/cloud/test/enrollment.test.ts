import { describe, expect, test } from "bun:test";
import { checkCode, generateCode, hashCode, issueToken } from "../src/domain/enrollment";

const NOW = 1_000_000;

function token() {
  return issueToken({
    organizationId: "org-1",
    branchId: "branch-1",
    createdBy: "user-1",
    nowUnixMs: NOW,
  });
}

describe("enrollment-коды", () => {
  test("код проходит проверку один раз", () => {
    const { code, token: value } = token();
    expect(checkCode(code, value, NOW + 1000).ok).toBe(true);

    value.usedAtUnixMs = NOW + 1000;
    const second = checkCode(code, value, NOW + 2000);
    expect(second.ok).toBe(false);
    expect(second.ok === false && second.error).toBe("ENROLLMENT_ERROR_CODE_ALREADY_USED");
  });

  test("истёкший код отклоняется", () => {
    const { code, token: value } = token();
    const result = checkCode(code, value, value.expiresAtUnixMs);
    expect(result.ok === false && result.error).toBe("ENROLLMENT_ERROR_CODE_EXPIRED");
  });

  test("чужой код отклоняется", () => {
    const { token: value } = token();
    const result = checkCode(generateCode(), value, NOW + 1000);
    expect(result.ok === false && result.error).toBe("ENROLLMENT_ERROR_CODE_INVALID");
  });

  test("отсутствующий токен отклоняется", () => {
    const result = checkCode("ABCD2345", undefined, NOW);
    expect(result.ok === false && result.error).toBe("ENROLLMENT_ERROR_CODE_INVALID");
  });

  test("код нечувствителен к регистру и пробелам", () => {
    const { code, token: value } = token();
    expect(checkCode(` ${code.toLowerCase()} `, value, NOW + 1).ok).toBe(true);
  });

  test("в хранилище попадает только хеш кода", () => {
    const { code, token: value } = token();
    expect(value).not.toHaveProperty("code");
    expect(value.codeHash.equals(hashCode(code))).toBe(true);
  });

  test("алфавит кода не содержит путающихся символов", () => {
    for (let index = 0; index < 200; index += 1) {
      expect(generateCode()).not.toMatch(/[01OI]/);
    }
  });
});
