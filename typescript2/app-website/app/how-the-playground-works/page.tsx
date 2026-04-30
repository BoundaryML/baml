import type { Metadata } from 'next';
import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

export const metadata: Metadata = {
  description:
    'The BAML playground runs entirely in your browser. Learn how BAML compiles to WebAssembly so you can edit, typecheck, and run BAML code without a backend.',
  title: 'How the BAML Playground Works',
};

export default function HowThePlaygroundWorks() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col min-h-screen w-full px-6 md:px-12 py-16 md:py-24">
        <article className="prose prose-neutral max-w-3xl mx-auto">
          <h1 className="text-4xl md:text-5xl font-semibold tracking-tight mb-6">
            How the playground works
          </h1>
          <p className="text-lg text-muted-foreground mb-10">
            The playground you just used is running the real BAML compiler, not
            a simulation, not a server. Everything happens in your browser.
          </p>

          <h2 className="text-2xl font-semibold mt-10 mb-4">
            BAML is written in Rust
          </h2>
          <p className="mb-6">
            The BAML compiler, the parser, type checker, prompt renderer, and
            runtime, is written in Rust. Rust was chosen so the same code can
            power the CLI, the VSCode extension, the Python/TypeScript/Ruby
            SDKs, and this playground without rewriting the language for each
            host.
          </p>

          <h2 className="text-2xl font-semibold mt-10 mb-4">
            Rust → WebAssembly
          </h2>
          <p className="mb-6">
            We compile the Rust crates to WebAssembly using{' '}
            <code className="px-1.5 py-0.5 rounded bg-muted">wasm-bindgen</code>
            . The result is a single{' '}
            <code className="px-1.5 py-0.5 rounded bg-muted">.wasm</code> module
            plus a thin JavaScript wrapper that exposes the compiler to the
            browser as ordinary JS functions. WebAssembly runs at near-native
            speed and ships with a strong sandbox, so the compiler can parse and
            typecheck your code without any server round-trip.
          </p>

          <h2 className="text-2xl font-semibold mt-10 mb-4">
            What happens when you type
          </h2>
          <ul className="list-disc pl-6 space-y-2 mb-6">
            <li>Your BAML source lives in an in-memory virtual file system.</li>
            <li>
              Every keystroke feeds the source into the WASM compiler, which
              parses it, resolves types, and returns diagnostics: the same ones
              you would see from the CLI.
            </li>
            <li>
              When you hit <strong>Run</strong>, the compiler renders the prompt
              for the selected function and calls the LLM provider directly from
              your browser using the API key you supply. No BAML server sees
              your keys or your data.
            </li>
            <li>
              The structured response is parsed by the same schema-aware parser
              that powers the SDKs, so what you see here is exactly what you
              would get in production.
            </li>
          </ul>

          <h2 className="text-2xl font-semibold mt-10 mb-4">
            Why this matters
          </h2>
          <p className="mb-10">
            Because the playground runs the same compiler as the SDKs, there is
            no drift between what works here and what works in your app. If it
            typechecks in the browser, it typechecks in CI.
          </p>

          <div className="flex gap-4 items-center mt-12">
            <Link
              className="text-[13px] tracking-[0.15em] uppercase text-[#6D28D9] hover:opacity-80 transition-opacity"
              href="/"
            >
              ← Back to the playground
            </Link>
          </div>
        </article>
        <FooterSection />
      </main>
    </div>
  );
}
