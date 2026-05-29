import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { notification } from "antd";
import { ApiError, isApiError } from "../api";

function notifyApiError(error: unknown, scope: "query" | "mutation") {
  if (!isApiError(error)) {
    notification.error({
      message: scope === "mutation" ? "Mutation failed" : "Request failed",
      description: error instanceof Error ? error.message : String(error),
    });
    return;
  }

  // 401 走 router redirect 到 /login，不重复 toast 打扰用户
  if (error.status === 401) return;

  notification.error({
    message: error.title,
    description: error.detail,
  });
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: (failureCount, error) => {
          if (error instanceof ApiError && error.status === 401) return false;
          return failureCount < 1;
        },
        staleTime: 30_000,
        refetchOnWindowFocus: false,
      },
      mutations: {
        retry: 0,
      },
    },
    queryCache: new QueryCache({
      onError: (error) => notifyApiError(error, "query"),
    }),
    mutationCache: new MutationCache({
      onError: (error) => notifyApiError(error, "mutation"),
    }),
  });
}

export const queryClient = createQueryClient();
