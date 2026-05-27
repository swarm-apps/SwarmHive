export interface ProblemBody {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
  required_permission?: string;
  scope?: string;
}

export class ApiError extends Error {
  readonly type: string;
  readonly title: string;
  readonly status: number;
  readonly detail?: string;
  readonly instance?: string;
  readonly required_permission?: string;
  readonly scope?: string;

  constructor(init: Required<Pick<ApiError, "title" | "status">> & Partial<ProblemBody>) {
    super(init.title);
    this.name = "ApiError";
    this.type = init.type ?? "about:blank";
    this.title = init.title;
    this.status = init.status;
    this.detail = init.detail;
    this.instance = init.instance;
    this.required_permission = init.required_permission;
    this.scope = init.scope;
  }
}

export function isApiError(value: unknown): value is ApiError {
  return value instanceof ApiError;
}

export async function parseProblemJson(response: Response): Promise<ApiError> {
  const contentType = response.headers.get("content-type") ?? "";
  if (contentType.includes("application/problem+json")) {
    try {
      const body = (await response.json()) as ProblemBody;
      return new ApiError({
        type: body.type ?? "about:blank",
        title: body.title ?? `HTTP ${response.status}`,
        status: body.status ?? response.status,
        detail: body.detail,
        instance: body.instance,
        required_permission: body.required_permission,
        scope: body.scope,
      });
    } catch {
      // fall through to fallback
    }
  }

  return new ApiError({
    title: `HTTP ${response.status}`,
    status: response.status,
  });
}
