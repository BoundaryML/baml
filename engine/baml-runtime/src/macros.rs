#[macro_use]
macro_rules! internal_feature {
  () => {
      #[cfg(feature = "internal")]
      { pub }
      #[cfg(not(feature = "internal"))]
      { pub(crate) }
  };
}
