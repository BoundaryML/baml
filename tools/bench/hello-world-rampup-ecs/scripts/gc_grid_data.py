"""Merge retained GC-grid evidence sources."""


def merge_data(base, current):
    cells = {(float(cell['gc_frequency_hz']), int(cell['rate'])): cell for cell in base['cells']}
    cells.update({(float(cell['gc_frequency_hz']), int(cell['rate'])): cell for cell in current['cells']})
    frequencies = sorted({float(value) for source in (base, current) for value in source['metadata']['gc_frequencies_hz']})
    rates = sorted({int(cell['rate']) for cell in cells.values()})
    runs = []
    for source in (base, current):
        for run in source['metadata'].get('runs', [source['metadata']['run']]):
            if run not in runs:
                runs.append(run)
    metadata = {**current['metadata'], 'rates_rps': rates, 'gc_frequencies_hz': frequencies,
                'adaptive': True, 'missing_cells': [], 'runs': runs}
    frontier = {float(event['gc_frequency_hz']): event for source in (base, current) for event in source.get('frontier', [])}
    return {**current, 'metadata': metadata, 'cells': sorted(cells.values(), key=lambda cell: (cell['gc_frequency_hz'], cell['rate'])),
            'app_stops': base.get('app_stops', []) + current.get('app_stops', []),
            'frontier': [frontier[key] for key in sorted(frontier)]}
