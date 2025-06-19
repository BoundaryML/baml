module Internal
  
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOU_RE_DOING_RUNTIME = T.let(
    Baml::Ffi::BamlRuntime.from_files("baml_src", Baml::Internal::FILE_MAP, ENV),
    Baml::Ffi::BamlRuntime
  )

  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOU_RE_DOING_CTX = T.let(
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOU_RE_DOING_RUNTIME.create_context_manager(),
    Baml::Ffi::RuntimeContextManager
  )
end