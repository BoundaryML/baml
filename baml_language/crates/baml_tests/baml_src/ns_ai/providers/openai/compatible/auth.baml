class NoAuth {}

class BearerAuth {
  token: string,
}

class HeaderAuth {
  name: string,
  value: string,
}

type Auth = NoAuth | BearerAuth | HeaderAuth
