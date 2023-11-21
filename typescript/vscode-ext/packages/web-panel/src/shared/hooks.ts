import { useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { ASTContext } from './ASTProvider'

export function useSelections() {
  const ctx = useContext(ASTContext)
  if (!ctx) {
    throw new Error('useSelections must be used within an ASTProvider')
  }
  const {
    db,
    test_results: test_results_raw,
    selections: { selectedFunction, selectedImpl, selectedTestCase },
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
  const test_results = useMemo(
    () => test_results_raw.filter((tr) => tr.functionName == func?.name.value),
    [test_results_raw, func?.name.value],
  )

  return {
    func,
    impl,
    test_case,
    test_results,
  }
}

export function useImplCtx(name: string) {
  const { func, test_results, test_case } = useSelections()

  const impl_test_result = useMemo(
    () =>
      test_results.find(
        (tr) =>
          tr.implName == name &&
          (test_case?.name.value ? tr.testName == test_case.name.value : tr.testName == 'playground'),
      ),
    [test_results, name],
  )

  return { func, test_result: impl_test_result }
}
