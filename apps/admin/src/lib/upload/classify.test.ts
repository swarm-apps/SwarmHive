import { describe, expect, it } from "vitest";
import { classifyArtifact, isSignatureFile, pairSignatures } from "./classify";

describe("classifyArtifact", () => {
  it("classifies APK as Android and extracts ABI", () => {
    expect(classifyArtifact("app-arm64-v8a-release.apk")).toEqual({
      platform: "react-native-android",
      kind: "universal",
      abi: "arm64-v8a",
      uncertain: false,
    });
    expect(classifyArtifact("app-armeabi-v7a.apk").abi).toBe("armeabi-v7a");
  });

  it("prefers x86_64 over x86 in ABI detection", () => {
    expect(classifyArtifact("app-x86_64.apk").abi).toBe("x86_64");
    expect(classifyArtifact("app-x86.apk").abi).toBe("x86");
  });

  it("classifies APK without a recognized ABI as Android with undefined abi", () => {
    expect(classifyArtifact("app-universal.apk")).toEqual({
      platform: "react-native-android",
      kind: "universal",
      abi: undefined,
      uncertain: false,
    });
  });

  it("classifies desktop bundles as Tauri", () => {
    for (const name of [
      "Foo_0.4.5_x64-setup.exe",
      "Foo_0.4.5_x64.msi",
      "Foo.dmg",
      "Foo.app.tar.gz",
      "Foo.AppImage",
      "Foo.AppImage.tar.gz",
      "foo_amd64.deb",
      "foo.x86_64.rpm",
      "Foo_0.4.5_x64-setup.nsis.zip",
    ]) {
      expect(classifyArtifact(name)).toMatchObject({
        platform: "tauri-desktop",
        uncertain: false,
      });
    }
  });

  it("classifies desktop artifact roles", () => {
    expect(classifyArtifact("Foo.dmg").kind).toBe("installer");
    expect(classifyArtifact("Foo.app.tar.gz").kind).toBe("updater");
    expect(classifyArtifact("Foo_0.4.5_x64-setup.exe").kind).toBe("universal");
  });

  it("flags unknown extensions as uncertain (defaulting to tauri-desktop)", () => {
    expect(classifyArtifact("mystery.bin")).toEqual({
      platform: "tauri-desktop",
      kind: "universal",
      uncertain: true,
    });
  });
});

describe("isSignatureFile", () => {
  it("detects .sig regardless of case", () => {
    expect(isSignatureFile("Foo.app.tar.gz.sig")).toBe(true);
    expect(isSignatureFile("Foo.app.tar.gz.SIG")).toBe(true);
    expect(isSignatureFile("Foo.app.tar.gz")).toBe(false);
  });
});

describe("pairSignatures", () => {
  it("pairs a .sig with its sibling bundle", () => {
    const r = pairSignatures(["Foo.app.tar.gz", "Foo.app.tar.gz.sig"]);
    expect(r.bundles).toEqual(["Foo.app.tar.gz"]);
    expect(r.signatureByBundle).toEqual({ "Foo.app.tar.gz": "Foo.app.tar.gz.sig" });
    expect(r.orphanSignatures).toEqual([]);
  });

  it("reports an orphan .sig with no matching bundle", () => {
    const r = pairSignatures(["Foo.msi", "Bar.app.tar.gz.sig"]);
    expect(r.bundles).toEqual(["Foo.msi"]);
    expect(r.signatureByBundle).toEqual({});
    expect(r.orphanSignatures).toEqual(["Bar.app.tar.gz.sig"]);
  });

  it("handles a batch with no signatures", () => {
    const r = pairSignatures(["a.apk", "b.msi"]);
    expect(r.bundles).toEqual(["a.apk", "b.msi"]);
    expect(r.signatureByBundle).toEqual({});
    expect(r.orphanSignatures).toEqual([]);
  });
});
