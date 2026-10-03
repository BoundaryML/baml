enum Status {
    Active,
    Inactive,
}

class Customer {
    name: string,
    status: Status,
}

class Box<T> {
    value: T,
}

interface Named {
    name: string,
}

type CustomerOrStatus = Customer | Status;

type Tree = int | Tree[];

function EnumArg(status: Status) -> Status {
    status
}

function OptionalEnum(status: Status?) -> Status? {
    status
}

function ClassArg(customer: Customer) -> Customer {
    customer
}

function Nested(values: map<string, Box<Status>[]>) -> map<string, Box<Status>[]> {
    values
}

function UnionArg(value: CustomerOrStatus) -> CustomerOrStatus {
    value
}

function RecursiveArg(value: Tree) -> Tree {
    value
}

function InterfaceArg(value: Named) -> Named {
    value
}

function GenericBox<T>(value: Box<T>) -> Box<T> {
    value
}
