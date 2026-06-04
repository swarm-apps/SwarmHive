import { docs } from "collections/server";
import { loader } from "fumadocs-core/source";
import { docsContentRoute, docsImageRoute, docsRoute } from "./shared";
import { BASE_PATH } from "./site";

// See https://fumadocs.dev/docs/headless/source-api for more info
export const source = loader({
  baseUrl: docsRoute,
  source: docs.toFumadocsSource(),
  plugins: [],
});

export function getPageImage(page: (typeof source)["$inferPage"]) {
  const segments = [...page.slugs, "image.png"];

  return {
    segments,
    // OG 图绝对解析靠 metadataBase(纯 origin),故 url 这里补 basePath 前缀
    url: `${BASE_PATH}${docsImageRoute}/${segments.join("/")}`,
  };
}

export function getPageMarkdownUrl(page: (typeof source)["$inferPage"]) {
  const segments = [...page.slugs, "content.md"];

  return {
    segments,
    url: `${docsContentRoute}/${segments.join("/")}`,
  };
}

export async function getLLMText(page: (typeof source)["$inferPage"]) {
  const processed = await page.data.getText("processed");

  return `# ${page.data.title} (${page.url})

${processed}`;
}
