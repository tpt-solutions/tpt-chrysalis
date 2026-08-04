//! The typed environment snapshot the gate evaluates rules against.

use alloc::collections::BTreeMap;

use crate::rule::Operator;

/// A physical/chemical variable the gate reasons about. Extending this enum
/// extends the rule language (Phase 2 adds organoid-specific cross-chip vars).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Var {
    /// Ambient/fluid temperature in milli-degrees Celsius.
    TemperatureMilliC,
    /// Manifold pressure in millibar.
    PressureMbar,
    /// pH scaled x1000.
    PhMilli,
    /// Dispense volume in microlitres.
    VolumeUl,
    /// Presence (1.0) / absence (0.0) of reagent A.
    ReagentA,
    /// Presence (1.0) / absence (0.0) of reagent B.
    ReagentB,
    /// Presence (1.0) / absence (0.0) of reagent C.
    ReagentC,
}

impl Var {
    /// The stable symbolic name, used in audit/report output.
    pub const fn name(self) -> &'static str {
        match self {
            Var::TemperatureMilliC => "temperature_milli_c",
            Var::PressureMbar => "pressure_mbar",
            Var::PhMilli => "ph_milli",
            Var::VolumeUl => "volume_ul",
            Var::ReagentA => "reagent_a",
            Var::ReagentB => "reagent_b",
            Var::ReagentC => "reagent_c",
        }
    }
}

/// A numeric environment value. Reagent presence is encoded as `0.0`/`1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvValue(pub f64);

impl EnvValue {
    pub const fn num(v: f64) -> Self {
        Self(v)
    }

    pub const fn present() -> Self {
        Self(1.0)
    }

    pub const fn absent() -> Self {
        Self(0.0)
    }
}

/// A snapshot of the conditions a protocol expects to hold. A missing variable
/// is *not* zero — the gate treats it as unknown and fails closed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Environment {
    values: BTreeMap<Var, EnvValue>,
}

impl Environment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, var: Var, value: EnvValue) -> &mut Self {
        self.values.insert(var, value);
        self
    }

    /// Reads a variable. `None` means *unknown* — the gate aborts on it.
    pub fn get(&self, var: Var) -> Option<EnvValue> {
        self.values.get(&var).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The set of variables this environment knows about.
    pub fn vars(&self) -> impl Iterator<Item = Var> + '_ {
        self.values.keys().copied()
    }

    /// True iff every variable of `env` is present here with the same value.
    pub fn covers(&self, other: &Environment) -> bool {
        other
            .values
            .iter()
            .all(|(var, val)| self.values.get(var) == Some(val))
    }
}

impl core::fmt::Display for Environment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut first = true;
        for (var, val) in &self.values {
            if !first {
                f.write_str(", ")?;
            }
            first = false;
            write!(f, "{}={}", var.name(), val.0)?;
        }
        Ok(())
    }
}

/// Evaluates `op(lhs, rhs)` over finite floats. All operators are total on
/// finite inputs, so the gate never panics.
pub(crate) fn cmp(op: Operator, lhs: f64, rhs: f64) -> bool {
    match op {
        Operator::Lt => lhs < rhs,
        Operator::Le => lhs <= rhs,
        Operator::Gt => lhs > rhs,
        Operator::Ge => lhs >= rhs,
        Operator::Eq => lhs == rhs,
        Operator::Ne => lhs != rhs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_covers_subset() {
        let mut full = Environment::new();
        full.set(Var::TemperatureMilliC, EnvValue::num(4_000.0))
            .set(Var::ReagentA, EnvValue::present());
        let mut subset = Environment::new();
        subset.set(Var::ReagentA, EnvValue::present());
        assert!(full.covers(&subset));
        assert!(!subset.covers(&full));
    }

    #[test]
    fn missing_var_is_unknown_not_zero() {
        let env = Environment::new();
        assert_eq!(env.get(Var::TemperatureMilliC), None);
    }
}
