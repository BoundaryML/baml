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
        u: User => u.name,
        e: ErrorInfo => e.message
    };

    let age = match (input) {
        u: User => u.age,
        e: ErrorInfo => -e.code
    };

    let role_score = match (input) {
        u: User => match (u.role) {
            Role.Admin => 10,
            Role.Editor => 5,
            Role.Viewer => 2,
            Role.Guest => 1
        },
        e: ErrorInfo => 0
    };

    let base = age * 2 + 10;
    let capped = if (base > 100) {
        100
    } else if (base < 0) {
        0
    } else {
        base
    };
    let bonus = if (capped % 2 == 0) { 5 } else { 0 };
    let total = capped * role_score + bonus;

    let suffix = match (total > threshold) {
        true => " [HIGH]",
        false => " [LOW]"
    };
    let label = name + suffix;

    let scores = [base, capped, total];
    scores[0] = total;
    let top = scores[0];

    let breakdown = { "base": base, "role": role_score };

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
