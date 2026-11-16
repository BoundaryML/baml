class User {
    name: string,
    age: int,
}

class Admin {
    handle: string,
}

function f(u: User | Admin) -> string {
    let User { name, age } = u else {
        return "admin";
    };
    name
}
