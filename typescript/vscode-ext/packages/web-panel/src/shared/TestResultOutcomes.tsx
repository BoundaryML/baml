import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useSelections } from './hooks'
import { DataTable } from './TestResults/data-table'
import { columns } from './TestResults/columns'

const TestResultPanel = () => {
  const { test_results } = useSelections()

  return <DataTable columns={columns} data={test_results} />
}

export default TestResultPanel
