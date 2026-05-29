import { type components, fetchClient } from ".";

// ────────────────────────── Schema 派生类型 ──────────────────────────

export type PresignFile = components["schemas"]["PresignFile"];
export type PresignResponse = components["schemas"]["PresignResponse"];
export type PresignPart = components["schemas"]["PresignPart"];
export type CompletePart = components["schemas"]["CompletePart"];
export type CompleteResponse = components["schemas"]["CompleteResponse"];

// ────────────────────────── 路径常量 ──────────────────────────

const PRESIGN_PATH = "/api/v1/apps/{slug}/releases/{version}/uploads/presign" as const;
const COMPLETE_PATH =
  "/api/v1/apps/{slug}/releases/{version}/uploads/{upload_id}/complete" as const;

// ────────────────────────── presign / complete ──────────────────────────

export async function presignUpload(
  slug: string,
  version: string,
  files: PresignFile[],
): Promise<PresignResponse> {
  const { data, error } = await fetchClient.POST(PRESIGN_PATH, {
    params: { path: { slug, version } },
    body: { files },
  });
  if (error) throw error;
  if (!data) throw new Error("presign response missing body");
  return data;
}

export async function completeUpload(
  slug: string,
  version: string,
  uploadId: string,
  parts: CompletePart[],
  publish: boolean,
): Promise<CompleteResponse> {
  const { data, error } = await fetchClient.POST(COMPLETE_PATH, {
    params: { path: { slug, version, upload_id: uploadId } },
    body: { parts, publish },
  });
  if (error) throw error;
  if (!data) throw new Error("complete response missing body");
  return data;
}

// ────────────────────────── 直传对象存储(XHR + 进度) ──────────────────────────
// 浏览器自动设置的头无法手动覆盖(host / content-length),设了会被静默忽略且产生
// 控制台告警,这里直接跳过;Content-MD5 / x-amz-checksum-sha256 等签名头原样回放。
const FORBIDDEN_HEADERS = new Set(["host", "content-length", "connection"]);

/**
 * PUT 一个文件到预签名 URL,原样回放 server 返回的 headers。用 XMLHttpRequest 而非
 * fetch——只有 XHR 暴露 `upload.onprogress`。网络层失败大概率是桶未配 CORS。
 */
export function putToStorage(
  part: PresignPart,
  blob: Blob,
  onProgress?: (ratio: number) => void,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("PUT", part.presigned_url, true);
    for (const [k, v] of Object.entries(part.headers)) {
      if (!FORBIDDEN_HEADERS.has(k.toLowerCase())) xhr.setRequestHeader(k, v);
    }
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable) onProgress?.(e.loaded / e.total);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) resolve();
      else reject(new Error(`PUT ${xhr.status}: ${xhr.responseText || "upload rejected"}`));
    };
    xhr.onerror = () =>
      reject(new Error("upload network error — is the storage bucket CORS configured?"));
    xhr.send(blob);
  });
}
