function FieldAccess(user: User) -> string {
  // Simple field access
  let name = user.name;

  // Nested field access
  let bio = user.profile.bio;

  // Deep nesting
  let theme = user.profile.settings.theme;

  // Field access then array index
  let firstTag = user.tags[0];

  return theme;
}

class User {
  name string
  profile Profile
  tags string[]
}

class Profile {
  bio string
  settings Settings
}

class Settings {
  theme string
}
