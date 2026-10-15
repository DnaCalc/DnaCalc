//! The package for CalcFunc: the worksheet function catalogue and semantic kernels.
//!
//! Dependency rule: it never depends on calc-fml, calc-engine or calc-host; formatting is to reach it through a lower-level service contract.
//!
//! Its value API is the calc-value package, exposed as the module [`value`]: identities, checked
//! grid coordinates, worksheet values and the numeric policy.

pub use calc_value as value;
