interface PublicIdentity {
  type Key
  key: Key
}

class AccountRecord {
  public_key: string

  implements PublicIdentity {
    type Key = string
    key as public_key
  }
}

interface Repository {
  type Record
  function find(self) -> Self.Record
}

class UserRecord {
  name: string
}

class UserRepository {
  value: UserRecord

  implements Repository {
    type Record = UserRecord

    function find(self) -> Self.Record {
      return self.value
    }
  }
}

function example(account: AccountRecord, repo: UserRepository) -> void {
  let publicKey<[CURSOR] = account.as<PublicIdentity>.key
  let found<[CURSOR] = repo.find()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at associated_type_projection_metadata.baml:37:16
// ```baml
// publicKey: string
// ```
// hover at associated_type_projection_metadata.baml:38:12
// ```baml
// found: UserRecord
// ```
