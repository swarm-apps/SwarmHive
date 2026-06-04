import { notFound } from "next/navigation";
import { DEMO_NAMES, type DemoName } from "@/components/demo/demo-names";
import { DemoStage } from "@/components/demo/demo-stage";

// 每个 demo 一张独立静态页,供文档里的 <ComponentPreview> 经 iframe 隔离加载。
export function generateStaticParams() {
  return DEMO_NAMES.map((name) => ({ name }));
}

export default async function PreviewPage({ params }: PageProps<"/preview/[name]">) {
  const { name } = await params;
  if (!DEMO_NAMES.includes(name as DemoName)) notFound();
  return <DemoStage name={name as DemoName} />;
}
