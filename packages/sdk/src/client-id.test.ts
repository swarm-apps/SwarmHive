import { describe, expect, it } from "vitest";
import { ensureClientId } from "./client-id";
import { memStorage } from "./test-utils";

describe("ensureClientId", () => {
  it("generates once and returns the same id thereafter", async () => {
    const storage = memStorage();
    let n = 0;
    const gen = () => `id-${n++}`;
    const a = await ensureClientId(storage, gen);
    const b = await ensureClientId(storage, gen);
    expect(a).toBe("id-0");
    expect(b).toBe("id-0"); // stable across calls
    expect(n).toBe(1); // generator invoked only once
  });
});
