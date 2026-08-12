// Testing all the formatter cases for chain expressions
// including paths, field access, calls, index, and permutations of these

class ChainUser {
    name string
    profile ChainProfile
    tags string[]
    scores int[][]
}

class ChainProfile {
    bio string
    settings ChainSettings
    items ChainItem[]
}

class ChainSettings {
    theme string
    notifications bool
    preferences ChainPreferences
}

class ChainPreferences {
    color string
    font string
    size int
}

class ChainItem {
    label string
    value int
    metadata ChainMetadata
}

class ChainMetadata {
    created_at string
    tags string[]
}

function ChainExprs(user: ChainUser, items: ChainItem[], m: map<string, int>) -> string {

    // ===== Simple path =====
    user;
    user.name;
    user.profile.bio;
    user.profile.settings.theme;

    // ===== Deep path exceeding limit =====
    user.profile.settings.preferences.color;
    user.profile.items[0].metadata.tags[0];
    user.profile.settings.preferences.font;

    // ===== Trivia in path (line comments) =====
    // 1 first leading
    user // 2 first trailing
    // 3 dot leading
    . // 4 dot trailing
    // 5 field leading
    name // 6 field trailing
    ;

    // ===== Trivia in path (block comments) =====
    /* 1 first leading */
    user /* 2 first trailing */
    /* 3 dot leading */
    . /* 4 dot trailing */
    /* 5 field leading */
    name /* 6 field trailing */
    ;

    // ===== Trivia in deep path (line comments) =====
    // 1 root leading
    user // 2 root trailing
    . // 3 dot1 trailing
    // 4 profile leading
    profile // 5 profile trailing
    . // 6 dot2 trailing
    // 7 settings leading
    settings // 8 settings trailing
    . // 9 dot3 trailing
    // 10 theme leading
    theme // 11 theme trailing
    ;

    // ===== Function calls =====
    user.name.len();
    items.len();

    // ===== Call with arguments =====
    user.profile.items.get(0);

    // ===== Trivia in call (line comments) =====
    // 1 callee leading
    items // 2 callee trailing
    // 3 open paren leading
    . // 4 dot trailing
    // 5 method leading
    get // 6 method trailing
    ( // 7 open paren trailing
    // 8 arg leading
    0 // 9 arg trailing
    // 10 close paren leading
    ) // 11 close paren trailing
    ;

    // ===== Trivia in call (block comments) =====
    /* 1 callee leading */
    items /* 2 callee trailing */
    /* 3 dot leading */
    . /* 4 dot trailing */
    /* 5 method leading */
    get /* 6 method trailing */
    ( /* 7 open paren trailing */
    /* 8 arg leading */
    0 /* 9 arg trailing */
    /* 10 close paren leading */
    ) /* 11 close paren trailing */
    ;

    // ===== Index access =====
    items[0];
    items[0].label;
    user.tags[0];
    user.scores[0][1];

    // ===== Trivia in index (line comments) =====
    // 1 base leading
    items // 2 base trailing
    // 3 open bracket leading
    [ // 4 open bracket trailing
    // 5 index leading
    0 // 6 index trailing
    // 7 close bracket leading
    ] // 8 close bracket trailing
    ;

    // ===== Trivia in index (block comments) =====
    /* 1 base leading */
    items /* 2 base trailing */
    /* 3 open bracket leading */
    [ /* 4 open bracket trailing */
    /* 5 index leading */
    0 /* 6 index trailing */
    /* 7 close bracket leading */
    ] /* 8 close bracket trailing */
    ;

    // ===== Chained calls =====
    user.name.len().to_string();

    // ===== Long chain exceeding limit: many field accesses =====
    user.profile.settings.preferences.color.len().to_string().len().to_string().len().to_string().len().to_string();

    // ===== Long chain exceeding limit: deep path + call + index =====
    user.profile.items[0].metadata.tags[0].len().to_string().len().to_string().len().to_string();

    // ===== Long call args exceeding limit =====
    user.profile.items.get(user.profile.items.len() - 1 + user.profile.items.len() - 2 + user.profile.items.len() - 3).label;

    // ===== Call with many args exceeding limit =====
    user.profile.items.slice(0, user.profile.items.len() - 1, user.profile.settings.preferences.size);

    // ===== Trivia in long wrapping chain (line comments) =====
    // 1 leading
    user // 2 trailing
    . // 3 trailing
    // 4 leading
    profile // 5 trailing
    . // 6 trailing
    // 7 leading
    settings // 8 trailing
    . // 9 trailing
    // 10 leading
    preferences // 11 trailing
    . // 12 trailing
    // 13 leading
    color // 14 trailing
    . // 15 trailing
    // 16 leading
    len // 17 trailing
    ( // 18 trailing
    ) // 19 trailing
    . // 20 trailing
    // 21 leading
    to_string // 22 trailing
    ( // 23 trailing
    ) // 24 trailing
    . // 25 trailing
    // 26 leading
    len // 27 trailing
    ( // 28 trailing
    ) // 29 trailing
    ;

    // ===== Trivia in long wrapping chain (block comments) =====
    /* 1 leading */
    user /* 2 trailing */
    . /* 3 trailing */
    /* 4 leading */
    profile /* 5 trailing */
    . /* 6 trailing */
    /* 7 leading */
    settings /* 8 trailing */
    . /* 9 trailing */
    /* 10 leading */
    preferences /* 11 trailing */
    . /* 12 trailing */
    /* 13 leading */
    color /* 14 trailing */
    . /* 15 trailing */
    /* 16 leading */
    len /* 17 trailing */
    ( /* 18 trailing */
    ) /* 19 trailing */
    . /* 20 trailing */
    /* 21 leading */
    to_string /* 22 trailing */
    ( /* 23 trailing */
    ) /* 24 trailing */
    . /* 25 trailing */
    /* 26 leading */
    len /* 27 trailing */
    ( /* 28 trailing */
    ) /* 29 trailing */
    ;

    // ===== Trivia in call with long args (line comments) =====
    // 1 leading
    user // 2 trailing
    . // 3 trailing
    // 4 leading
    profile // 5 trailing
    . // 6 trailing
    // 7 leading
    items // 8 trailing
    . // 9 trailing
    // 10 leading
    get // 11 trailing
    ( // 12 trailing
    // 13 leading
    user.profile.items.len() - 1 + user.profile.items.len() - 2 // 14 trailing
    ) // 15 trailing
    . // 16 trailing
    // 17 leading
    label // 18 trailing
    ;

    // ===== Deeply nested index chains =====
    user.scores[0][0];
    user.scores[0][1];
    user.scores[user.scores.len() - 1][user.scores[0].len() - 1];

    // ===== Extraneous whitespace =====
    user  .  name;
    items  [  0  ];
    items  .  get  (  0  );

    // ===== Mixed chain: field + index + call =====
    user.tags[0].len();
    user.scores[0][0];
    items[0].label.len();

    // ===== Env access =====
    env.API_KEY;
    env.DATABASE_URL;

    // ===== Final expression =====
    user.name
}
