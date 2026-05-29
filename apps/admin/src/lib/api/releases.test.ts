import { describe, expect, it } from "vitest";
import {
  canPublish,
  canYank,
  publishedVersions,
  type Release,
  releaseStatusColor,
} from "./releases";

function makeRelease(version: string, status: Release["status"]): Release {
  return {
    id: `id-${version}`,
    app_id: "app1",
    version,
    android_version_code: null,
    status,
    release_notes: null,
    published_at: null,
    created_at: "2026-05-01T00:00:00Z",
    updated_at: "2026-05-01T00:00:00Z",
  };
}

describe("release 状态判断 helper", () => {
  it("canPublish 仅 draft 为 true", () => {
    expect(canPublish("draft")).toBe(true);
    expect(canPublish("published")).toBe(false);
    expect(canPublish("yanked")).toBe(false);
  });

  it("canYank 仅 published 为 true", () => {
    expect(canYank("published")).toBe(true);
    expect(canYank("draft")).toBe(false);
    expect(canYank("yanked")).toBe(false);
  });

  it("releaseStatusColor 映射 published→green / yanked→red / draft→default", () => {
    expect(releaseStatusColor("published")).toBe("green");
    expect(releaseStatusColor("yanked")).toBe("red");
    expect(releaseStatusColor("draft")).toBe("default");
  });
});

describe("publishedVersions", () => {
  it("只保留 published 版本", () => {
    const list = [
      makeRelease("1.0.0", "published"),
      makeRelease("1.1.0", "draft"),
      makeRelease("0.9.0", "yanked"),
      makeRelease("1.2.0", "published"),
    ];
    expect(publishedVersions(list).map((r) => r.version)).toEqual(["1.0.0", "1.2.0"]);
  });

  it("空列表返回空", () => {
    expect(publishedVersions([])).toEqual([]);
  });
});
