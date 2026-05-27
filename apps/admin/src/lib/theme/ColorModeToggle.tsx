import { DesktopOutlined, MoonOutlined, SunOutlined } from "@ant-design/icons";
import { Trans } from "@lingui/react/macro";
import { Segmented, Tooltip } from "antd";
import { useColorModeContext } from "./ColorModeProvider";
import type { ColorMode } from "./useColorMode";

const OPTIONS: { value: ColorMode; icon: React.ReactNode; label: React.ReactNode }[] = [
  {
    value: "light",
    icon: <SunOutlined />,
    label: <Trans>浅色</Trans>,
  },
  {
    value: "dark",
    icon: <MoonOutlined />,
    label: <Trans>深色</Trans>,
  },
  {
    value: "system",
    icon: <DesktopOutlined />,
    label: <Trans>跟随系统</Trans>,
  },
];

export function ColorModeToggle() {
  const { mode, setMode } = useColorModeContext();

  return (
    <Tooltip title={<Trans>切换主题</Trans>}>
      <Segmented<ColorMode>
        value={mode}
        onChange={setMode}
        options={OPTIONS.map((opt) => ({
          value: opt.value,
          icon: opt.icon,
          title: typeof opt.label === "string" ? opt.label : undefined,
        }))}
      />
    </Tooltip>
  );
}
