//! Evaluation profile policy, determinism, and output permissions.

use lyma_runtime::RuntimeLimits;
use lyma_syntax::{
    Diagnostic, DiagnosticCode, LymaKey, LymaMapping, LymaSequence, LymaTaggedValue, LymaValue,
    Severity,
};

/// Determinism handling for evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeterministicMode {
    /// Reject nondeterministic runtime outputs.
    #[default]
    Enforced,
    /// Permit nondeterministic runtime outputs when other policy bits allow them.
    Permissive,
}

/// Host output permissions for runtime-originated values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RuntimeOutputPolicy {
    /// Whether runtime-originated function values may escape evaluation.
    pub allow_function_values: bool,
    /// Whether runtime-originated userdata values may escape evaluation.
    pub allow_user_data: bool,
    /// Whether runtime-originated host objects may escape evaluation.
    pub allow_host_objects: bool,
}

/// Evaluation profile used by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationProfile {
    /// Stable host-facing profile name.
    pub name: &'static str,
    /// Determinism mode enforced for runtime values.
    pub deterministic_mode: DeterministicMode,
    /// Runtime-output permissions.
    pub runtime_output: RuntimeOutputPolicy,
    /// Runtime resource limits to apply during evaluation.
    pub runtime_limits: RuntimeLimits,
}

impl Default for EvaluationProfile {
    fn default() -> Self {
        Self::restricted()
    }
}

impl EvaluationProfile {
    /// Returns the deny-by-default restricted evaluation profile.
    #[must_use]
    pub fn restricted() -> Self {
        Self {
            name: "restricted",
            deterministic_mode: DeterministicMode::Enforced,
            runtime_output: RuntimeOutputPolicy::default(),
            runtime_limits: RuntimeLimits::sandboxed(),
        }
    }

    /// Returns a permissive profile with caller-supplied runtime limits.
    #[must_use]
    pub const fn permissive(runtime_limits: RuntimeLimits) -> Self {
        Self {
            name: "permissive",
            deterministic_mode: DeterministicMode::Permissive,
            runtime_output: RuntimeOutputPolicy {
                allow_function_values: true,
                allow_user_data: true,
                allow_host_objects: true,
            },
            runtime_limits,
        }
    }
}

/// Shared deny-by-default evaluation profile.
pub static RESTRICTED_EVALUATION_PROFILE: EvaluationProfile = EvaluationProfile {
    name: "restricted",
    deterministic_mode: DeterministicMode::Enforced,
    runtime_output: RuntimeOutputPolicy {
        allow_function_values: false,
        allow_user_data: false,
        allow_host_objects: false,
    },
    runtime_limits: RuntimeLimits::sandboxed(),
};

/// Host policy abstraction for evaluation profiles.
pub trait ProfilePolicy {
    /// Returns the active evaluation profile.
    fn profile(&self) -> &EvaluationProfile;

    /// Validates runtime-originated output against the active profile.
    ///
    /// # Errors
    /// Returns [`ProfileError`] when the output contains forbidden runtime values
    /// or nondeterministic host keys under deterministic mode.
    fn validate_runtime_output(&self, value: &LymaValue) -> Result<(), ProfileError> {
        validate_value(self.profile(), value)
    }
}

impl ProfilePolicy for EvaluationProfile {
    fn profile(&self) -> &EvaluationProfile {
        self
    }
}

/// Stable profile validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    /// Stable diagnostic describing the profile violation.
    pub diagnostic: Diagnostic,
}

impl ProfileError {
    /// Creates a stable profile validation error.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let mut diagnostic = Diagnostic::new(code, Severity::Error);
        diagnostic.message = message.into();
        Self { diagnostic }
    }
}

fn validate_value(profile: &EvaluationProfile, value: &LymaValue) -> Result<(), ProfileError> {
    match value {
        LymaValue::Null(_)
        | LymaValue::Boolean(_)
        | LymaValue::Number(_)
        | LymaValue::String(_) => Ok(()),
        LymaValue::Sequence(LymaSequence { items, .. }) => items
            .iter()
            .try_for_each(|item| validate_value(profile, item)),
        LymaValue::Mapping(LymaMapping { entries, .. }) => {
            for entry in entries {
                validate_key(profile, &entry.key)?;
                validate_value(profile, &entry.value)?;
            }
            Ok(())
        }
        LymaValue::Tagged(LymaTaggedValue { value, .. }) => validate_value(profile, value),
        LymaValue::Function(_) if profile.runtime_output.allow_function_values => Ok(()),
        LymaValue::Function(_) => Err(ProfileError::new(
            DiagnosticCode::FunctionValueNotAllowedInThisProfile,
            format!("profile '{}' forbids function values", profile.name),
        )),
        LymaValue::UserData(_) if profile.runtime_output.allow_user_data => Ok(()),
        LymaValue::UserData(_) => Err(ProfileError::new(
            DiagnosticCode::UnsupportedProfile,
            format!("profile '{}' forbids userdata values", profile.name),
        )),
        LymaValue::HostObject(_) if profile.runtime_output.allow_host_objects => Ok(()),
        LymaValue::HostObject(_) => Err(ProfileError::new(
            DiagnosticCode::UnsupportedProfile,
            format!("profile '{}' forbids host object values", profile.name),
        )),
    }
}

fn validate_key(profile: &EvaluationProfile, key: &LymaKey) -> Result<(), ProfileError> {
    match key {
        LymaKey::String(_) | LymaKey::Number(_) | LymaKey::Boolean(_) => Ok(()),
        LymaKey::Host(_)
            if profile.deterministic_mode == DeterministicMode::Permissive
                && profile.runtime_output.allow_host_objects =>
        {
            Ok(())
        }
        LymaKey::Host(_) => Err(ProfileError::new(
            DiagnosticCode::NonDeterministicTableIteration,
            format!(
                "profile '{}' forbids host keys in deterministic mode",
                profile.name
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use lyma_syntax::{LymaHostValue, LymaMapping, LymaMappingEntry, LymaNull};

    use super::{DeterministicMode, EvaluationProfile, ProfilePolicy, RuntimeOutputPolicy};

    #[test]
    fn restricted_profile_denies_runtime_host_values() {
        let profile = EvaluationProfile::restricted();
        let error = profile
            .validate_runtime_output(&lyma_syntax::LymaValue::Function(LymaHostValue {
                kind: String::from("lua_fn"),
                label: Some(String::from("fn")),
            }))
            .expect_err("restricted profile should reject function values");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::FunctionValueNotAllowedInThisProfile
        );
    }

    #[test]
    fn deterministic_mode_rejects_host_mapping_keys() {
        let profile = EvaluationProfile {
            name: "strict",
            deterministic_mode: DeterministicMode::Enforced,
            runtime_output: RuntimeOutputPolicy {
                allow_host_objects: true,
                ..RuntimeOutputPolicy::default()
            },
            runtime_limits: lyma_runtime::RuntimeLimits::sandboxed(),
        };
        let error = profile
            .validate_runtime_output(&lyma_syntax::LymaValue::Mapping(LymaMapping {
                entries: vec![LymaMappingEntry {
                    key: lyma_syntax::LymaKey::Host(LymaHostValue {
                        kind: String::from("host"),
                        label: None,
                    }),
                    value: lyma_syntax::LymaValue::Null(LymaNull),
                    span: None,
                }],
                duplicate_keys: Vec::new(),
                span: None,
            }))
            .expect_err("deterministic mode should reject host keys");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::NonDeterministicTableIteration
        );
    }
}
