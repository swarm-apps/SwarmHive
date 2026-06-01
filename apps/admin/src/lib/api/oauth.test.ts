import { describe, expect, it } from "vitest";
import { oauthLinkStartUrl, oauthLoginUrl } from "./oauth";

describe("oauthLoginUrl", () => {
  it("encodes the next path so query chars survive the redirect", () => {
    expect(oauthLoginUrl("github", "/apps")).toBe("/api/v1/auth/oauth/github/start?next=%2Fapps");
    // A next that itself carries a query (e.g. the device page) must be fully encoded.
    expect(oauthLoginUrl("github", "/device?user_code=WDJB-MJHT")).toBe(
      "/api/v1/auth/oauth/github/start?next=%2Fdevice%3Fuser_code%3DWDJB-MJHT",
    );
  });
});

describe("oauthLinkStartUrl", () => {
  it("targets the link-start endpoint for the kind", () => {
    expect(oauthLinkStartUrl("github")).toBe("/api/v1/auth/oauth/providers/link/github/start");
  });
});
