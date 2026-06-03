import { describe, expect, it } from "vitest";
import { semverComparator, versionCodeComparator } from "./compare";
import { mockRelease } from "./test-utils";

describe("semverComparator", () => {
  it("newer is an update", () => {
    expect(semverComparator("0.4.0", mockRelease({ version: "0.4.5" }))).toBe(true);
  });
  it("tolerates a single leading v", () => {
    expect(semverComparator("v0.4.0", mockRelease({ version: "0.4.5" }))).toBe(true);
    expect(semverComparator("0.4.0", mockRelease({ version: "v0.4.5" }))).toBe(true);
  });
  it("equal is not an update", () => {
    expect(semverComparator("0.4.5", mockRelease({ version: "0.4.5" }))).toBe(false);
  });
  it("older candidate is not an update", () => {
    expect(semverComparator("0.5.0", mockRelease({ version: "0.4.5" }))).toBe(false);
  });
  it("invalid semver is not an update", () => {
    expect(semverComparator("0.4.0", mockRelease({ version: "latest" }))).toBe(false);
  });
  it("handles pre-release versions", () => {
    // 正式版 > 预发布;预发布 < 正式版(semver 规则)
    expect(semverComparator("0.4.5-alpha", mockRelease({ version: "0.4.5" }))).toBe(true);
    expect(semverComparator("0.4.5", mockRelease({ version: "0.4.5-beta" }))).toBe(false);
  });
});

describe("versionCodeComparator", () => {
  it("higher versionCode is an update", () => {
    expect(versionCodeComparator("18", mockRelease({ versionCode: 21 }))).toBe(true);
  });
  it("equal versionCode is not an update", () => {
    expect(versionCodeComparator("21", mockRelease({ versionCode: 21 }))).toBe(false);
  });
  it("missing versionCode is not an update", () => {
    expect(versionCodeComparator("18", mockRelease({}))).toBe(false);
  });
  it("non-integer current string is not an update", () => {
    expect(versionCodeComparator("18.5", mockRelease({ versionCode: 21 }))).toBe(false);
  });
});
