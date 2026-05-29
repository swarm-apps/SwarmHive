import { describe, expect, it } from "vitest";
import { type ApiToken, type PermissionName, permissionLabel, tokenStatus } from "./tokens";

function token(overrides: Partial<ApiToken>): ApiToken {
  return {
    id: "00000000-0000-0000-0000-000000000001",
    owner_user_id: "00000000-0000-0000-0000-000000000002",
    kind: "api",
    name: "ci",
    prefix: "swhv_api_AbC",
    permissions: ["release:publish"],
    last_used_at: null,
    expires_at: null,
    revoked_at: null,
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("tokenStatus", () => {
  it("is revoked when revoked_at is set (even if also expired)", () => {
    expect(
      tokenStatus(
        token({ revoked_at: "2026-01-02T00:00:00Z", expires_at: "2020-01-01T00:00:00Z" }),
      ),
    ).toBe("revoked");
  });

  it("is expired when expires_at is in the past and not revoked", () => {
    expect(tokenStatus(token({ expires_at: "2020-01-01T00:00:00Z" }))).toBe("expired");
  });

  it("is active when neither revoked nor past expiry", () => {
    expect(tokenStatus(token({}))).toBe("active");
    expect(tokenStatus(token({ expires_at: "2999-01-01T00:00:00Z" }))).toBe("active");
  });
});

describe("permissionLabel", () => {
  it("maps known permissions to friendly labels", () => {
    expect(permissionLabel("release:publish")).toBe("Publish releases");
    expect(permissionLabel("artifact:upload")).toBe("Upload artifacts");
  });

  it("falls back to the wire string for unknown permissions", () => {
    expect(permissionLabel("future:permission" as PermissionName)).toBe("future:permission");
  });
});
