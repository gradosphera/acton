#[cfg(test)]
pub mod assertions;
#[cfg(test)]
pub mod compilation;
#[cfg(test)]
pub mod fixtures;
#[cfg(test)]
pub mod project;
#[cfg(test)]
pub mod snapshots;

#[allow(unused_imports)]
pub use assertions::TestOutputExt;
