#[cfg(feature = "allocation_profiling")]
mod allocation_stats;
mod bex_str;
#[cfg(test)]
mod tests;

#[cfg(feature = "allocation_profiling")]
pub use allocation_stats::{BexStrAllocationStats, allocation_stats};
pub use bex_str::{BexStr, FlatStr};
