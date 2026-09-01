import { redirect } from "next/navigation"

import { referencePackagePath } from "@/lib/reference-data"

export default function SelectorlessBamlPackagePage() {
  redirect(referencePackagePath("latest", "baml"))
}
