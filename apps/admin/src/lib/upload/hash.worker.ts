// Web Worker:用 hash-wasm 流式算文件的 MD5 + SHA256(hex)。按 8MB 分块喂,避免大
// 文件(数百 MB)在主线程算 hash 卡住 UI。presign 需要 Content-MD5(必绑)+ SHA256
// (后端支持时绑),二者一次遍历同时算出。
//
// 用 Comlink(GoogleChromeLabs)把 worker API 包成普通 async 调用,省掉手写
// postMessage/onmessage 协议;进度回调由主线程用 Comlink.proxy 包后跨边界回调。

import * as Comlink from "comlink";
import { createMD5, createSHA256 } from "hash-wasm";

const CHUNK = 8 * 1024 * 1024;

export interface FileHashes {
  md5: string;
  sha256: string;
}

const api = {
  async hash(file: File, onProgress?: (ratio: number) => void): Promise<FileHashes> {
    const md5 = await createMD5();
    const sha256 = await createSHA256();
    md5.init();
    sha256.init();
    let offset = 0;
    while (offset < file.size) {
      const end = Math.min(offset + CHUNK, file.size);
      const buf = new Uint8Array(await file.slice(offset, end).arrayBuffer());
      md5.update(buf);
      sha256.update(buf);
      offset = end;
      // fire-and-forget:进度只是 UI 反馈,不阻塞 hash 等回调 round-trip。
      onProgress?.(file.size > 0 ? offset / file.size : 1);
    }
    return { md5: md5.digest("hex"), sha256: sha256.digest("hex") };
  },
};

export type HashWorkerApi = typeof api;

Comlink.expose(api);
