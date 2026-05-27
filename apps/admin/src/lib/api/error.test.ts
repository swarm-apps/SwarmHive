import { describe, expect, it } from "vitest";
import { ApiError, isApiError, parseProblemJson } from "./error";

function makeResponse(body: unknown, init: { status: number; contentType: string }): Response {
  return new Response(typeof body === "string" ? body : JSON.stringify(body), {
    status: init.status,
    headers: { "content-type": init.contentType },
  });
}

describe("parseProblemJson", () => {
  it("parses application/problem+json into structured ApiError", async () => {
    const response = makeResponse(
      {
        type: "https://swarmhive.dev/errors/forbidden",
        title: "Forbidden",
        status: 403,
        detail: "Missing permission",
        required_permission: "app:create",
        scope: "global",
      },
      { status: 403, contentType: "application/problem+json" },
    );

    const err = await parseProblemJson(response);

    expect(isApiError(err)).toBe(true);
    expect(err.status).toBe(403);
    expect(err.title).toBe("Forbidden");
    expect(err.detail).toBe("Missing permission");
    expect(err.required_permission).toBe("app:create");
    expect(err.scope).toBe("global");
  });

  it("falls back to 'HTTP N' title when content-type is not problem+json", async () => {
    const response = makeResponse("upstream failure", {
      status: 500,
      contentType: "text/plain",
    });

    const err = await parseProblemJson(response);

    expect(isApiError(err)).toBe(true);
    expect(err.status).toBe(500);
    expect(err.title).toBe("HTTP 500");
    expect(err.detail).toBeUndefined();
  });

  it("falls back when problem+json body is malformed JSON", async () => {
    const response = makeResponse("not-json", {
      status: 422,
      contentType: "application/problem+json",
    });

    const err = await parseProblemJson(response);

    expect(err.status).toBe(422);
    expect(err.title).toBe("HTTP 422");
  });
});

describe("isApiError", () => {
  it("returns true only for ApiError instances", () => {
    expect(isApiError(new ApiError({ title: "x", status: 400 }))).toBe(true);
    expect(isApiError(new Error("boom"))).toBe(false);
    expect(isApiError(null)).toBe(false);
    expect(isApiError({ status: 401 })).toBe(false);
  });
});

describe("ApiError.extra", () => {
  it("returns typed extras attached to the problem body (e.g. locked_until)", async () => {
    const response = makeResponse(
      {
        type: "https://swarmhive.dev/errors/account-locked-until",
        title: "Account temporarily locked",
        status: 410,
        detail: "Too many failed attempts.",
        locked_until: "2026-05-27T15:00:00Z",
      },
      { status: 410, contentType: "application/problem+json" },
    );

    const err = await parseProblemJson(response);

    expect(err.type).toBe("https://swarmhive.dev/errors/account-locked-until");
    expect(err.extra<string>("locked_until")).toBe("2026-05-27T15:00:00Z");
    expect(err.extra<string>("not_present")).toBeUndefined();
  });

  it("returns extras from typed bootstrap problems", async () => {
    const response = makeResponse(
      {
        type: "https://swarmhive.dev/errors/bootstrap-email-mismatch",
        title: "Bootstrap email mismatch",
        status: 422,
        detail: "pinned",
        expected_email: "owner@example.com",
      },
      { status: 422, contentType: "application/problem+json" },
    );

    const err = await parseProblemJson(response);
    expect(err.extra<string>("expected_email")).toBe("owner@example.com");
  });
});
