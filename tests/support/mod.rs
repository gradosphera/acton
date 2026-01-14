#[cfg(test)]
pub(crate) mod assertions;
#[cfg(test)]
pub(crate) mod compilation;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
pub(crate) mod project;
#[cfg(test)]
pub(crate) mod snapshots;

#[allow(unused_imports)]
pub(crate) use assertions::TestOutputExt;
