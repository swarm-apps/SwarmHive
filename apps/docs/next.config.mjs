import { createMDX } from "fumadocs-mdx/next";

const withMDX = createMDX();

// GitHub Pages 子路径前缀：仅 CI 部署时经 PAGES_BASE_PATH 注入（本地 build 留空便于验证）
const basePath = process.env.PAGES_BASE_PATH ?? "";

/** @type {import('next').NextConfig} */
const config = {
  output: "export",
  reactStrictMode: true,
  basePath,
  // basePath 不会自动前缀裸 <iframe src>，显式暴露给客户端拼 /preview/* 地址
  env: { NEXT_PUBLIC_BASE_PATH: basePath },
  // 静态导出无图片优化 server
  images: { unoptimized: true },
  // 避免 GitHub Pages 目录路由 404
  trailingSlash: true,
  // pnpm workspace 包要 Next 转译
  transpilePackages: ["@swarm-hive/sdk"],
};

export default withMDX(config);
