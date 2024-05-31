import type { StringSpan, TestResult, TestState } from '@baml/common'
import { useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { ASTContext } from './ASTProvider'

type JSONSchema = {
  [key: string]: any
  definitions?: { [key: string]: any }
}

function removeUnreferencedDefinitions(schema: JSONSchema): JSONSchema {
  if (!schema || typeof schema !== 'object' || !schema.definitions) {
    return schema
  }

  // Function to collect references from a given object
  function collectRefs(obj: any, refs: Set<string>) {
    if (obj && typeof obj === 'object') {
      for (const key of Object.keys(obj)) {
        if (key === '$ref' && typeof obj[key] === 'string') {
          // Extract and store the reference
          const ref = obj[key].replace('#/definitions/', '')
          refs.add(ref)
        } else {
          // Recursively collect references from nested objects
          collectRefs(obj[key], refs)
        }
      }
    }
  }

  // Initialize a set to keep track of all referenced definitions
  const referencedDefs = new Set<string>()

  // Collect references from the entire schema, excluding the definitions object itself
  collectRefs({ ...schema, definitions: {} }, referencedDefs)

  // Iterate over the definitions to find and include indirectly referenced definitions
  let newlyAdded: boolean
  do {
    newlyAdded = false
    for (const def of Object.keys(schema.definitions)) {
      if (referencedDefs.has(def)) {
        const initialSize = referencedDefs.size
        collectRefs(schema.definitions[def], referencedDefs)
        if (referencedDefs.size > initialSize) {
          newlyAdded = true
        }
      }
    }
  } while (newlyAdded)

  // Filter out definitions that are not referenced
  const newDefinitions = Object.keys(schema.definitions)
    .filter((def) => referencedDefs.has(def))
    .reduce(
      (newDefs, def) => {
        if (schema.definitions) {
          newDefs[def] = schema.definitions[def]
        }
        return newDefs
      },
      {} as { [key: string]: any },
    )

  return { ...schema, definitions: newDefinitions }
}
function useSelections() {
  const ctx = useContext(ASTContext)
  if (!ctx) {
    throw new Error('useSelections must be used within an ASTProvider')
  }
  const {
    db,
    test_results: test_results_raw,
    jsonSchema,
    test_log,
    selections: { selectedFunction, selectedImpl, selectedTestCase, showTests },
  } = ctx

  const func = useMemo(() => {
    if (!selectedFunction) {
      return db.functions.at(0)
    }
    return db.functions.find((f) => f.name.value === selectedFunction)
  }, [db.functions, selectedFunction])
  const impl = useMemo(() => {
    if (!func) {
      return undefined
    }
    if (!selectedImpl) {
      return func.impls.at(0)
    }
    return func.impls.find((i) => i.name.value === selectedImpl)
  }, [func, selectedImpl])
  const test_case = useMemo(() => {
    if (selectedTestCase === '<new>') {
      return undefined
    }
    return func?.test_cases.find((t) => t.name.value === selectedTestCase) ?? func?.test_cases.at(0)
  }, [func, selectedTestCase])

  // TODO: we should just publish a global test status instead of relying
  // on this exit code.
  const test_result_exit_status: TestState['run_status'] = useMemo(() => {
    if (!test_results_raw) return 'NOT_STARTED'
    return test_results_raw.run_status
  }, [test_results_raw, func?.name.value])
  const test_result_url = useMemo(() => {
    if (!test_results_raw) return undefined
    if (test_results_raw.test_url) {
      return { text: 'Dashboard', url: test_results_raw.test_url }
    } else {
      return {
        text: 'Learn how to persist runs',
        url: 'https://docs.boundaryml.com/v2/mdx/quickstart#setting-up-the-boundary-dashboard',
      }
    }
  }, [test_results_raw, func?.name.value])

  const renderedTestCase = useMemo(() => {
    const tc = func?.impls.flatMap((i) => i.prompt.test_case).filter((tc): tc is string => tc !== undefined)
    if (!tc || tc.length === 0) return undefined
    return tc[0]
  }, [func])

  const test_results: (TestResult[] & { span?: StringSpan }) | undefined = useMemo(() => {
    return test_results_raw?.results
      .filter((tr) => tr.functionName == func?.name.value)
      .map((tr) => {
        const relatedTest = func?.test_cases.find((tc) => tc.name.value == tr.testName)
        return {
          ...tr,
          input: relatedTest?.content,
          span: relatedTest?.name,
        }
      })
  }, [test_results_raw, func?.name.value])

  const input_json_schema = useMemo(() => {
    if (!func) return undefined

    const base_schema = {
      title: `${func.name.value} Input`,
      ...jsonSchema,
    }

    let merged_schema = {}

    if (func.input.arg_type === 'named') {
      merged_schema = {
        type: 'object',
        properties: Object.fromEntries(func.input.values.map((v) => [v.name.value, v.jsonSchema])),
        ...base_schema,
      }
    } else {
      merged_schema = {
        ...func.input.jsonSchema,
        ...base_schema,
      }
    }

    return removeUnreferencedDefinitions(merged_schema)
  }, [func, jsonSchema])

  return {
    func,
    impl,
    showTests,
    test_case,
    test_results,
    test_result_url,
    test_result_exit_status,
    test_log,
    input_json_schema,
    renderedTestCase,
  }
}

export function useImplCtx(name: string) {
  const { func } = useSelections()

  return { func }
}
