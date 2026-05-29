import { expect, test } from "@playwright/test";

test.describe("admin SPA smoke", () => {
  test("未登录访问 / 跳转 /login 并显示中文标题", async ({ page, context }) => {
    await context.clearCookies();
    await page.goto("/");

    await expect(page).toHaveURL(/\/login\?next=/);
    await expect(page.getByText("登录 SwarmHive")).toBeVisible();
  });

  test("login 页静态资源加载，含 zh-CN AntD locale 中文按钮", async ({ page }) => {
    await page.goto("/login?next=/");
    // disabled form 中的 placeholder 用了英文，但 antd 默认按钮 / Form label 应是中文文案
    await expect(page.getByText("邮箱")).toBeVisible();
    await expect(page.getByText("密码")).toBeVisible();
    await expect(page.getByText("登录表单尚未实现")).toBeVisible();
  });
});
