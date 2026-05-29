import createFetchClient, { type Middleware } from "openapi-fetch";
import createQueryClient from "openapi-react-query";
import { parseProblemJson } from "./error";
import type { paths } from "./schema.gen";

const errorMiddleware: Middleware = {
  async onResponse({ response }) {
    if (!response.ok) {
      throw await parseProblemJson(response.clone());
    }
    return response;
  },
};

export const fetchClient = createFetchClient<paths>({
  baseUrl: "/",
  credentials: "include",
  headers: { Accept: "application/json" },
});

fetchClient.use(errorMiddleware);

export const $api = createQueryClient(fetchClient);
