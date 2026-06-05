// with-android-install-permission —— Expo config plugin。
// 镜像生产 app(SwarmDrop-RN)的同名 plugin:prebuild 时只往 AndroidManifest 注入
// 一条 `REQUEST_INSTALL_PACKAGES` uses-permission。
//
// 方案 A(零原生代码)下,安装走 expo-intent-launcher 的 ACTION_VIEW + content URI
// (FileProvider 由 expo-file-system 的 getContentUriAsync 自带,本 plugin 不注入
//  FileProvider / file_paths.xml),故权限面只此一条。该权限是 signature|appop 级、
// 非 dangerous,运行时不能用 PermissionsAndroid.request() 拉起——用户授权走系统
// "安装未知应用"开关页(由 expo-intent-launcher 的安装 intent 触发)。
//
// 用法(app.json / app.config):plugins: ["./plugins/with-android-install-permission"]

const { withAndroidManifest } = require("@expo/config-plugins");

const PERMISSION = "android.permission.REQUEST_INSTALL_PACKAGES";

function injectPermission(manifest) {
  const root = manifest.manifest;
  root["uses-permission"] = root["uses-permission"] ?? [];

  const already = root["uses-permission"].some((p) => p.$ && p.$["android:name"] === PERMISSION);
  if (already) return manifest;

  root["uses-permission"].push({ $: { "android:name": PERMISSION } });
  return manifest;
}

const withAndroidInstallPermission = (config) =>
  withAndroidManifest(config, (config) => {
    config.modResults = injectPermission(config.modResults);
    return config;
  });

module.exports = withAndroidInstallPermission;
module.exports.injectPermission = injectPermission;
