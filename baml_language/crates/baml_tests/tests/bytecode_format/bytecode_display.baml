// User scoring system — exercises most bytecode instructions.

class User {
    name string
    age int
    role Role

    function promote(self, bonus: int) -> Report {
        self.age += 1;
        let base = score(self, bonus);
        let new_score = base.score + bonus;
        Report {
            score: new_score,
            label: base.label,
            breakdown: base.breakdown
        }
    }
}

class Report {
    score int
    label string
    breakdown map<string, int>
}

class ErrorInfo {
    code int
    message string
}

enum Role {
    Admin
    Editor
    Viewer
    Guest
}

type Input = User | ErrorInfo

// Exercises: load_var, load_field, store_var, bin_op (+, *, %), cmp_op (>, <, >=),
//   pop_jump_if_false, jump, unary_op (-), alloc_array, store_array_element,
//   load_array_element, alloc_map, alloc_instance, store_field, return,
//   discriminant, jump_table, type_tag, instanceof, copy, pop
function score(input: Input, threshold: int) -> Report {
    let name = match (input) {
        let u: User => u.name,
        let e: ErrorInfo => e.message
    };

    let age = match (input) {
        let u: User => u.age,
        let e: ErrorInfo => -e.code
    };

    let role_mult = match (input) {
        let u: User => match (u.role) {
            Role.Admin => threshold,
            Role.Editor => threshold,
            Role.Viewer => threshold,
            Role.Guest => threshold
        },
        let e: ErrorInfo => threshold
    };

    let base = age * 2 + role_mult;
    let capped = if (base > role_mult) {
        role_mult
    } else {
        base
    };
    let total = capped + role_mult;

    let suffix = match (total > threshold) {
        true => " [HIGH]",
        false => " [LOW]"
    };
    let label = name + suffix;

    let scores = [base, capped, total];
    scores[0] = total;
    let top = scores[0];

    let breakdown = { "base": base, "role": role_mult };

    Report {
        score: top,
        label: label,
        breakdown: breakdown
    }
}

// Exercises: match on int literals (binary search), string concatenation,
//   catch-all, load_const with various types
function http_status(code: int) -> string {
    let prefix = "HTTP ";
    let label = match (code) {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown"
    };
    prefix + label
}
