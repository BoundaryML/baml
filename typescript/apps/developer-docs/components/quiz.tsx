"use client"

import { useState } from "react"
import { CheckCircle2, CircleHelp, XCircle } from "lucide-react"

import { cn } from "@/lib/utils"

export interface QuizProps {
  question: string
  options: string[]
  answer: string
  explanation: string
}

export function Quiz({ question, options, answer, explanation }: QuizProps) {
  const [selected, setSelected] = useState<string>()
  const [checked, setChecked] = useState(false)
  const correct = selected === answer

  return (
    <section className="not-prose my-7 rounded-xl border bg-card p-4 shadow-sm" aria-label="Knowledge check">
      <div className="mb-3 flex items-start gap-2">
        <CircleHelp className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">Knowledge check</div>
          <h3 className="mt-1 text-sm font-semibold">{question}</h3>
        </div>
      </div>
      <div className="grid gap-2">
        {options.map((option) => (
          <label className={cn("flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-2.5 text-sm transition-colors hover:bg-accent", selected === option && "border-foreground bg-accent")} key={option}>
            <input checked={selected === option} name={question} onChange={() => { setSelected(option); setChecked(false) }} type="radio" value={option} />
            {option}
          </label>
        ))}
      </div>
      <button className="mt-3 inline-flex h-8 items-center rounded-md bg-foreground px-3 text-xs font-medium text-background disabled:opacity-50" disabled={!selected} onClick={() => setChecked(true)} type="button">
        Check answer
      </button>
      {checked ? (
        <div className={cn("mt-3 flex gap-2 rounded-lg px-3 py-2.5 text-sm", correct ? "bg-emerald-500/10 text-emerald-900 dark:text-emerald-200" : "bg-destructive/10 text-destructive")} role="status">
          {correct ? <CheckCircle2 className="mt-0.5 size-4 shrink-0" /> : <XCircle className="mt-0.5 size-4 shrink-0" />}
          <span><strong>{correct ? "Correct." : "Not quite."}</strong> {explanation}</span>
        </div>
      ) : null}
    </section>
  )
}
