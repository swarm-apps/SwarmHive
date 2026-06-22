import { expect, test } from "@playwright/test";

// E2E 跑在全新空库上:user 表为空时 admin SPA 把任意路径都路由到 /setup
//（Coolify 式首启 bootstrap;owner 建好前连 /login 也会被重定向到 /setup）。
// 两个测试都不提交表单,故库始终为空、两测都稳定停在 /setup。
test.describe("admin SPA smoke", () => {
  // i18n 默认按 navigator.language 检测(src/i18n.tsx detectLocale);CI 的 headless
  // chromium 是 en-US → 默认英文,而这两个 smoke 断言的是中文文案。先把 locale 钉成
  // zh-CN(localStorage "swarmhive.locale")让渲染确定性走中文(也正是「含 Lingui zh-CN」
  // 这条测试的本意)。addInitScript 在页面脚本前注入,早于 detectLocale 读 localStorage。
  // 用字符串形式而非函数:e2e tsconfig 不含 dom lib,函数体里的 window/localStorage
  // 会被 tsc 报 TS2304;字符串脚本不参与类型检查,运行时仍在浏览器执行。
  test.beforeEach(async ({ context }) => {
    await context.addInitScript('window.localStorage.setItem("swarmhive.locale", "zh-CN");');
  });

  test("空用户表下访问 / 跳转 /setup 并显示中文 bootstrap 标题", async ({ page, context }) => {
    await context.clearCookies();
    await page.goto("/");

    await expect(page).toHaveURL(/\/setup/);
    await expect(page.getByText("初始化 SwarmHive")).toBeVisible();
  });

  test("setup 页静态资源加载，含 Lingui zh-CN 中文表单", async ({ page }) => {
    await page.goto("/setup");
    // Lingui i18n 生效:中文副标题 + 中文提交按钮都渲染出来。
    // 用 button role + 完整按钮名定位,避免 getByText 命中多个含「密码」的 label。
    await expect(page.getByText("创建首位 Owner 账号", { exact: false })).toBeVisible();
    await expect(page.getByRole("button", { name: "创建 Owner 并登录" })).toBeVisible();
  });
});
