class Cell:
    __slots__ = ("value", "payload")

    def __init__(self, value, payload):
        self.value = value
        self.payload = payload


def main():
    batches, width, retain = 128, 2048, 4
    retained = []
    checksum = 0
    for batch in range(batches):
        cells = []
        for i in range(width):
            cell = Cell(batch + i, [i, i + 1, i + 2, i + 3])
            alias = cell
            alias.value += 1
            alias.payload[0] += 1
            cells.append(cell)
        for cell in cells:
            checksum += cell.value + cell.payload[0]
        if len(retained) < retain:
            retained.append(cells)
        else:
            retained[batch % retain] = cells
    live_sum = sum(cell.value + cell.payload[0] for cells in retained for cell in cells)
    return f"{checksum},{live_sum}"
