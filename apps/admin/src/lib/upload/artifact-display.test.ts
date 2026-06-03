import { describe, expect, it } from "vitest";
import { friendlyArch, platformRowSpans } from "./artifact-display";

describe("friendlyArch", () => {
  it("maps known Tauri target triples to friendly labels", () => {
    expect(friendlyArch("tauri-desktop", "aarch64-apple-darwin", null)).toBe("macOS Apple Silicon");
    expect(friendlyArch("tauri-desktop", "x86_64-apple-darwin", null)).toBe("macOS Intel");
    expect(friendlyArch("tauri-desktop", "x86_64-pc-windows-msvc", null)).toBe("Windows x64");
    expect(friendlyArch("tauri-desktop", "x86_64-unknown-linux-gnu", null)).toBe("Linux x64");
  });

  it("falls back to the raw triple for unknown Tauri targets", () => {
    expect(friendlyArch("tauri-desktop", "riscv64gc-unknown-linux-gnu", null)).toBe(
      "riscv64gc-unknown-linux-gnu",
    );
  });

  it("uses the raw abi for Android", () => {
    expect(friendlyArch("react-native-android", null, "arm64-v8a")).toBe("arm64-v8a");
    expect(friendlyArch("react-native-android", null, "armeabi-v7a")).toBe("armeabi-v7a");
  });

  it("falls back to — when nothing is present", () => {
    expect(friendlyArch("tauri-desktop", null, null)).toBe("—");
    expect(friendlyArch("react-native-android", null, null)).toBe("—");
  });
});

describe("platformRowSpans", () => {
  it("gives each platform segment's first row the segment length, the rest 0", () => {
    // 3 个 tauri-desktop + 1 个 android(已排序)
    const platforms = ["tauri-desktop", "tauri-desktop", "tauri-desktop", "react-native-android"];
    expect(platformRowSpans(platforms)).toEqual([3, 0, 0, 1]);
  });

  it("handles a single row and an empty list", () => {
    expect(platformRowSpans(["tauri-desktop"])).toEqual([1]);
    expect(platformRowSpans([])).toEqual([]);
  });

  it("handles alternating-free segments (each platform contiguous)", () => {
    expect(platformRowSpans(["a", "a", "b", "c", "c"])).toEqual([2, 0, 1, 2, 0]);
  });
});
