import Link from "next/link";

export function RunSignIn({ id = "" }: { id?: string }) {
  return <main className="max-w-3xl mx-auto p-8 space-y-4">
    <h1 className="text-2xl font-semibold">Bammy runs</h1>
    <p>Run prompts and transcripts are private. A BoundaryML/baml repository maintainer account is required to view them.</p>
    <Link className="underline" href={`/auth/github?run=${encodeURIComponent(id)}`}>Sign in with GitHub</Link>
  </main>;
}
