import { describe, expect, it } from "vitest";
import { deviceLoginNext } from "./device";

describe("deviceLoginNext", () => {
  it("embeds the user code as a query param", () => {
    expect(deviceLoginNext("WDJB-MJHT")).toBe("/device?user_code=WDJB-MJHT");
  });

  it("returns the bare path when no code is present", () => {
    expect(deviceLoginNext(undefined)).toBe("/device");
  });

  it("survives the login round-trip (encode → next → decode) without dropping the code", () => {
    // Mirrors login.tsx: next is built here, then split back into pathname +
    // search after a successful login.
    const next = deviceLoginNext("WDJB-MJHT");
    const url = new URL(next, "http://localhost");
    expect(url.pathname).toBe("/device");
    expect(Object.fromEntries(url.searchParams.entries())).toEqual({
      user_code: "WDJB-MJHT",
    });
  });
});
