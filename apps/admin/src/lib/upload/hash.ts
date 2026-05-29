// 主线程侧的 hash 入口:用 Comlink wrap 一个一次性 module worker 流式算
// MD5 + SHA256,进度回调用 Comlink.proxy 包后传进 worker;算完即 terminate。

import * as Comlink from "comlink";
import type { FileHashes, HashWorkerApi } from "./hash.worker";

export type { FileHashes };

export function hashFile(file: File, onProgress?: (ratio: number) => void): Promise<FileHashes> {
  const worker = new Worker(new URL("./hash.worker.ts", import.meta.url), { type: "module" });
  const api = Comlink.wrap<HashWorkerApi>(worker);
  return api
    .hash(file, onProgress ? Comlink.proxy(onProgress) : undefined)
    .finally(() => worker.terminate());
}
