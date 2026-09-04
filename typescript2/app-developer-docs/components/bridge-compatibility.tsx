import { loadBridgeData } from '@/lib/content/bridges';

export async function BridgeCompatibility({
  id,
  view,
}: {
  id: string;
  view: 'compatibility' | 'types' | 'transitions' | 'gotchas';
}) {
  const bridge = await loadBridgeData(id);

  if (view === 'compatibility') {
    return (
      <table>
        <thead>
          <tr>
            <th>Concern</th>
            <th>TypeScript behavior</th>
          </tr>
        </thead>
        <tbody>
          {bridge.compatibility.map((row) => (
            <tr key={row.concern}>
              <td>{row.concern}</td>
              <td>{row.behavior}</td>
            </tr>
          ))}
        </tbody>
      </table>
    );
  }

  if (view === 'types') {
    return (
      <div className="overflow-x-auto">
        <table>
          <thead>
            <tr>
              <th>BAML</th>
              <th>TypeScript</th>
              <th>Notes</th>
            </tr>
          </thead>
          <tbody>
            {bridge.types.map((row) => (
              <tr key={row.baml}>
                <td>
                  <code>{row.baml}</code>
                </td>
                <td>
                  <code>{row.host}</code>
                </td>
                <td>{row.notes}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  const items = view === 'transitions' ? bridge.transitions : bridge.gotchas;
  return (
    <ul>
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  );
}
