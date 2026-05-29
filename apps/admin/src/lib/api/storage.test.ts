import { describe, expect, it } from "vitest";
import { STORAGE_PRESETS } from "./storage";

describe("STORAGE_PRESETS", () => {
  it("RustFS 用 path-style + 公开直链", () => {
    expect(STORAGE_PRESETS.rustfs.force_path_style).toBe(true);
    expect(STORAGE_PRESETS.rustfs.url_mode).toBe("public");
  });

  it("阿里云 OSS 用 virtual-hosted + 预签名", () => {
    expect(STORAGE_PRESETS.oss.force_path_style).toBe(false);
    expect(STORAGE_PRESETS.oss.url_mode).toBe("signed");
  });

  it("自定义 S3 默认 virtual-hosted + 预签名", () => {
    expect(STORAGE_PRESETS.custom.force_path_style).toBe(false);
    expect(STORAGE_PRESETS.custom.url_mode).toBe("signed");
  });

  it("每个预设都有 label 与 endpointHint 字段", () => {
    for (const key of ["rustfs", "oss", "custom"] as const) {
      expect(typeof STORAGE_PRESETS[key].label).toBe("string");
      expect(STORAGE_PRESETS[key].label.length).toBeGreaterThan(0);
      expect(typeof STORAGE_PRESETS[key].endpointHint).toBe("string");
    }
  });
});
