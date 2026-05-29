export interface ProblemBody {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
  required_permission?: string;
  scope?: string;
  /**
   * Server-side typed problems (`account-locked-until`,
   * `bootstrap-email-mismatch`, …) merge extra fields into the top-level
   * problem object. We keep the raw body on the error so call sites can
   * pull those fields with a typed lookup rather than re-parsing JSON.
   */
  [key: string]: unknown;
}

export class ApiError extends Error {
  readonly type: string;
  readonly title: string;
  readonly status: number;
  readonly detail?: string;
  readonly instance?: string;
  readonly required_permission?: string;
  readonly scope?: string;
  /** Full parsed problem+json body, including any non-standard fields. */
  readonly raw: ProblemBody;

  constructor(init: Required<Pick<ApiError, "title" | "status">> & ProblemBody) {
    super(init.title);
    this.name = "ApiError";
    this.type = init.type ?? "about:blank";
    this.title = init.title;
    this.status = init.status;
    this.detail = init.detail;
    this.instance = init.instance;
    this.required_permission = init.required_permission;
    this.scope = init.scope;
    this.raw = init;
  }

  /** Typed lookup for server-attached extras (e.g. `locked_until`). */
  extra<T = unknown>(key: string): T | undefined {
    return this.raw[key] as T | undefined;
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
        ...body,
        type: body.type ?? "about:blank",
        title: body.title ?? `HTTP ${response.status}`,
        status: body.status ?? response.status,
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
