import { queryOptions } from "@tanstack/react-query";
import { type components, fetchClient, isApiError } from ".";

// Per-app GitHub Release download-source config (add-github-release-source).
// One source per app; PUT is a create-or-replace upsert, DELETE is idempotent.
// `access_token` is write-only (never round-trips in a response); a blank token
// on update keeps the stored one — mirrors the oauth / storage secret pattern.

export type GithubSourceView = components["schemas"]["GithubSourceView"];
export type CreateGithubSourceRequest = components["schemas"]["CreateGithubSourceRequest"];

const SOURCE_PATH = "/api/v1/apps/{slug}/github-source" as const;

export function githubSourceQueryKey(slug: string) {
  return ["github-source", slug] as const;
}

/**
 * Read the app's GitHub source config. The endpoint 404s when no source is
 * configured (a normal, opt-in state), so we map that to `null` rather than
 * letting it surface as a global error toast. Any other failure still throws.
 */
export function githubSourceQueryOptions(slug: string) {
  return queryOptions({
    queryKey: githubSourceQueryKey(slug),
    enabled: slug.length > 0,
    queryFn: async (): Promise<GithubSourceView | null> => {
      try {
        const { data, error } = await fetchClient.GET(SOURCE_PATH, {
          params: { path: { slug } },
        });
        if (error) throw error;
        return data ?? null;
      } catch (err) {
        if (isApiError(err) && err.status === 404) return null;
        throw err;
      }
    },
  });
}

export async function putGithubSource(
  slug: string,
  body: CreateGithubSourceRequest,
): Promise<GithubSourceView> {
  const { data, error } = await fetchClient.PUT(SOURCE_PATH, {
    params: { path: { slug } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("put github source: missing body");
  return data;
}

export async function deleteGithubSource(slug: string): Promise<void> {
  const { error } = await fetchClient.DELETE(SOURCE_PATH, {
    params: { path: { slug } },
  });
  if (error) throw error;
}
