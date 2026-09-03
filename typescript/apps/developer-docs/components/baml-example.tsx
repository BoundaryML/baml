import { BamlExampleClient } from "@/components/baml-example-client"
import { loadBamlExample } from "@/lib/examples/load-example"

export async function BamlExample({ id }: { id: string }) {
  const example = await loadBamlExample(id)
  return <BamlExampleClient {...example} />
}
