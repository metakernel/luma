use std::collections::BTreeSet;

use lyma_runtime::{ConversionPolicy, LuaRuntimeError, LuaRuntimePhase};
use lyma_syntax::{
    LymaHostValue, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber, LymaSequence,
    LymaValue,
};
use omnilua::{AnyUserData, Lua, LuaError, MetaMethod, Table, UserData, UserDataMethods, Value};

use crate::{engine::OmniLuaValue, engine_name, limits::table_entry_limit_error};

#[derive(Debug, Clone, Default)]
pub(super) struct NullSentinel;

impl UserData for NullSentinel {
    fn add_meta_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_lua, _this, ()| {
            Ok(String::from("null"))
        });
    }
}

#[derive(Debug, Clone)]
pub(super) struct FrozenValueView {
    pub(crate) label: String,
    pub(crate) value: LymaValue,
}

impl UserData for FrozenValueView {
    fn add_meta_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: Value| {
            let Some(value) = lookup_frozen_value(&this.value, &key) else {
                return Ok(Value::Nil);
            };
            materialize_lyma_value(lua, value).map_err(|error| lua_runtime_error(&error))
        });
        methods.add_meta_method_mut(
            MetaMethod::NewIndex,
            |_lua, this, (_key, _value): (Value, Value)| -> omnilua::Result<()> {
                Err(LuaError::runtime(format_args!(
                    "attempt to mutate read-only value '{}'",
                    this.label
                ))
                .into())
            },
        );
        methods.add_meta_method(MetaMethod::Len, |_lua, this, ()| {
            let len = match &this.value {
                LymaValue::Sequence(sequence) => {
                    i64::try_from(sequence.items.len()).unwrap_or(i64::MAX)
                }
                LymaValue::Mapping(mapping) => {
                    i64::try_from(mapping.entries.len()).unwrap_or(i64::MAX)
                }
                _ => 0,
            };
            Ok(len)
        });
        methods.add_meta_method(MetaMethod::ToString, |_lua, this, ()| {
            Ok(this.label.clone())
        });
    }
}

#[derive(Debug, Clone)]
pub(super) struct ReadOnlyNamespace {
    pub(crate) label: String,
    pub(crate) entries: Vec<(String, Value)>,
}

impl UserData for ReadOnlyNamespace {
    fn add_meta_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_lua, this, key: Value| {
            let key = match key {
                Value::String(value) => value.to_str()?,
                _ => return Ok(Value::Nil),
            };
            Ok(this
                .entries
                .iter()
                .find(|(name, _)| name == &key)
                .map_or(Value::Nil, |(_, value)| value.clone()))
        });
        methods.add_meta_method_mut(
            MetaMethod::NewIndex,
            |_lua, this, (_key, _value): (Value, Value)| -> omnilua::Result<()> {
                Err(LuaError::runtime(format_args!(
                    "attempt to mutate read-only namespace '{}'",
                    this.label
                ))
                .into())
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_lua, this, ()| {
            Ok(this.label.clone())
        });
    }
}

pub(super) fn freeze_runtime_value(value: OmniLuaValue) -> Result<LymaValue, LuaRuntimeError> {
    match value {
        OmniLuaValue::Frozen(value) => Ok(value),
        OmniLuaValue::Live(value) => to_lyma_from_live(&value, &ConversionPolicy::default(), None),
    }
}

pub(super) fn thaw_runtime_value(lua: &Lua, value: &LymaValue) -> Result<Value, LuaRuntimeError> {
    materialize_lyma_value(lua, value)
}

pub(super) fn to_lyma_value(
    value: &OmniLuaValue,
    policy: &ConversionPolicy,
    max_table_entries: Option<usize>,
) -> Result<LymaValue, LuaRuntimeError> {
    match value {
        OmniLuaValue::Frozen(value) => Ok(value.clone()),
        OmniLuaValue::Live(value) => to_lyma_from_live(value, policy, max_table_entries),
    }
}

pub(super) fn materialize_lyma_value(
    lua: &Lua,
    value: &LymaValue,
) -> Result<Value, LuaRuntimeError> {
    match value {
        LymaValue::Null(_) => Ok(Value::UserData(lua.create_userdata(NullSentinel).map_err(
            |error| runtime_conversion_error(&error, "failed to create null sentinel", None),
        )?)),
        LymaValue::Boolean(value) => Ok(Value::Boolean(*value)),
        LymaValue::Number(LymaNumber::Integer(value)) => Ok(Value::Integer(*value)),
        LymaValue::Number(LymaNumber::Float(value)) => Ok(Value::Number(*value)),
        LymaValue::String(value) => {
            Ok(Value::String(lua.create_string(value).map_err(
                |error| runtime_conversion_error(&error, "failed to create string", None),
            )?))
        }
        LymaValue::Sequence(_) | LymaValue::Mapping(_) => Ok(Value::UserData(
            lua.create_userdata(FrozenValueView {
                label: String::from("lyma:frozen-value"),
                value: value.clone(),
            })
            .map_err(|error| {
                runtime_conversion_error(&error, "failed to create frozen view", None)
            })?,
        )),
        LymaValue::Tagged(tagged) => materialize_lyma_value(lua, &tagged.value),
        LymaValue::Function(_) => Err(conversion_error(
            "cannot materialize function placeholder into OmniLua",
            policy_span_none(),
        )),
        LymaValue::UserData(_) => Err(conversion_error(
            "cannot materialize userdata placeholder into OmniLua",
            policy_span_none(),
        )),
        LymaValue::HostObject(_) => Err(conversion_error(
            "cannot materialize host object placeholder into OmniLua",
            policy_span_none(),
        )),
    }
}

fn to_lyma_from_live(
    value: &Value,
    policy: &ConversionPolicy,
    max_table_entries: Option<usize>,
) -> Result<LymaValue, LuaRuntimeError> {
    let mut state = TableEntryState {
        max_entries: max_table_entries,
        seen_entries: 0,
        origin_span: policy.origin_span,
        active_tables: BTreeSet::new(),
    };
    live_value_to_lyma(value, policy, &mut state)
}

struct TableEntryState {
    max_entries: Option<usize>,
    seen_entries: usize,
    origin_span: Option<lyma_syntax::source::Span>,
    active_tables: BTreeSet<usize>,
}

impl TableEntryState {
    fn record_entries(&mut self, count: usize) -> Result<(), LuaRuntimeError> {
        self.seen_entries = self.seen_entries.saturating_add(count);
        if self.max_entries.is_some_and(|max| self.seen_entries > max) {
            return Err(table_entry_limit_error(self.origin_span));
        }
        Ok(())
    }

    fn enter_table(&mut self, table: &Table) -> Result<usize, LuaRuntimeError> {
        let pointer = table.to_pointer().map_err(|error| {
            runtime_conversion_error(
                &error,
                "failed to inspect Lua table identity",
                self.origin_span,
            )
        })?;
        if !self.active_tables.insert(pointer) {
            return Err(profile_error(
                lyma_syntax::DiagnosticCode::SerializationError,
                "cyclic Lua tables cannot be converted to deterministic Lyma data",
                self.origin_span,
            ));
        }
        Ok(pointer)
    }

    fn exit_table(&mut self, pointer: usize) {
        self.active_tables.remove(&pointer);
    }
}

fn live_value_to_lyma(
    value: &Value,
    policy: &ConversionPolicy,
    state: &mut TableEntryState,
) -> Result<LymaValue, LuaRuntimeError> {
    match value {
        Value::Nil => Ok(LymaValue::Null(LymaNull)),
        Value::Boolean(value) => Ok(LymaValue::Boolean(*value)),
        Value::Integer(value) => Ok(LymaValue::Number(LymaNumber::Integer(*value))),
        Value::Number(value) => Ok(LymaValue::Number(LymaNumber::Float(*value))),
        Value::String(value) => Ok(LymaValue::String(value.to_str().map_err(|error| {
            runtime_conversion_error(&error, "string is not valid UTF-8", policy.origin_span)
        })?)),
        Value::Function(_) => {
            if policy.allow_functions {
                Ok(LymaValue::Function(LymaHostValue {
                    kind: String::from("lua_function"),
                    label: Some(String::from("function")),
                }))
            } else {
                Err(function_profile_error(policy.origin_span))
            }
        }
        Value::UserData(userdata) => {
            if is_null_userdata(userdata) {
                return Ok(LymaValue::Null(LymaNull));
            }
            if let Some(value) = is_frozen_value_view(userdata) {
                return Ok(value);
            }
            if policy.allow_userdata {
                Ok(LymaValue::UserData(LymaHostValue {
                    kind: String::from("lua_userdata"),
                    label: Some(String::from("userdata")),
                }))
            } else {
                Err(profile_error(
                    lyma_syntax::DiagnosticCode::UnsupportedProfile,
                    "profile forbids userdata values",
                    policy.origin_span,
                ))
            }
        }
        Value::LightUserData(_) | Value::Thread(_) => {
            if policy.allow_host_objects {
                Ok(LymaValue::HostObject(LymaHostValue {
                    kind: String::from("lua_host_object"),
                    label: Some(String::from("host-object")),
                }))
            } else {
                Err(profile_error(
                    lyma_syntax::DiagnosticCode::UnsupportedProfile,
                    "profile forbids host object values",
                    policy.origin_span,
                ))
            }
        }
        Value::Table(table) => table_to_lyma(table, policy, state),
    }
}

fn table_to_lyma(
    table: &Table,
    policy: &ConversionPolicy,
    state: &mut TableEntryState,
) -> Result<LymaValue, LuaRuntimeError> {
    let pointer = state.enter_table(table)?;
    let pairs = table.raw_pairs().map_err(|error| {
        runtime_conversion_error(&error, "failed to iterate Lua table", policy.origin_span)
    })?;
    state.record_entries(pairs.len())?;
    if let Some(sequence) = try_sequence(&pairs, policy, state)? {
        state.exit_table(pointer);
        return Ok(sequence);
    }

    let mut entries = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        entries.push(LymaMappingEntry {
            key: key_to_lyma(&key, policy)?,
            value: live_value_to_lyma(&value, policy, state)?,
            span: None,
        });
    }
    let mapped = LymaValue::Mapping(LymaMapping {
        entries,
        duplicate_keys: Vec::new(),
        span: None,
    });
    state.exit_table(pointer);
    Ok(mapped)
}

fn try_sequence(
    pairs: &[(Value, Value)],
    policy: &ConversionPolicy,
    state: &mut TableEntryState,
) -> Result<Option<LymaValue>, LuaRuntimeError> {
    let mut indexed = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let index = match key {
            Value::Integer(value) if *value > 0 => usize::try_from(*value).ok(),
            Value::Number(value) => positive_integral_index(*value),
            _ => None,
        };
        let Some(index) = index else {
            return Ok(None);
        };
        indexed.push((index, value));
    }
    indexed.sort_by_key(|(index, _)| *index);
    if indexed
        .iter()
        .enumerate()
        .any(|(expected, (actual, _))| *actual != expected + 1)
    {
        return Ok(None);
    }
    let mut items = Vec::with_capacity(indexed.len());
    for (_, value) in indexed {
        items.push(live_value_to_lyma(value, policy, state)?);
    }
    Ok(Some(LymaValue::Sequence(LymaSequence {
        items,
        span: None,
    })))
}

fn key_to_lyma(key: &Value, policy: &ConversionPolicy) -> Result<LymaKey, LuaRuntimeError> {
    match key {
        Value::Boolean(value) => Ok(LymaKey::Boolean(*value)),
        Value::Integer(value) => Ok(LymaKey::Number(LymaNumber::Integer(*value))),
        Value::Number(value) => Ok(LymaKey::Number(LymaNumber::Float(*value))),
        Value::String(value) => Ok(LymaKey::String(value.to_str().map_err(|error| {
            runtime_conversion_error(&error, "mapping key is not valid UTF-8", policy.origin_span)
        })?)),
        Value::UserData(userdata) if is_null_userdata(userdata) => Err(profile_error(
            lyma_syntax::DiagnosticCode::InvalidNullKey,
            "null sentinel cannot be used as a mapping key",
            policy.origin_span,
        )),
        _ => {
            if policy.allow_host_objects {
                Ok(LymaKey::Host(LymaHostValue {
                    kind: String::from("lua_host_key"),
                    label: Some(String::from("host-key")),
                }))
            } else {
                Err(profile_error(
                    lyma_syntax::DiagnosticCode::NonDeterministicTableIteration,
                    "profile forbids non-deterministic host mapping keys",
                    policy.origin_span,
                ))
            }
        }
    }
}

pub(super) fn is_null_userdata(userdata: &AnyUserData) -> bool {
    userdata.borrow::<NullSentinel>().is_ok()
}

fn is_frozen_value_view(userdata: &AnyUserData) -> Option<LymaValue> {
    userdata
        .borrow::<FrozenValueView>()
        .ok()
        .map(|value| value.value.clone())
}

fn lookup_frozen_value<'a>(value: &'a LymaValue, key: &Value) -> Option<&'a LymaValue> {
    match value {
        LymaValue::Mapping(mapping) => mapping
            .entries
            .iter()
            .find(|entry| frozen_key_matches(&entry.key, key))
            .map(|entry| &entry.value),
        LymaValue::Sequence(sequence) => {
            let index = match key {
                Value::Integer(index) if *index > 0 => usize::try_from(*index - 1).ok(),
                Value::Number(index) => {
                    positive_integral_index(*index).and_then(|i| i.checked_sub(1))
                }
                _ => None,
            };
            index.and_then(|index| sequence.items.get(index))
        }
        _ => None,
    }
}

fn frozen_key_matches(key: &LymaKey, candidate: &Value) -> bool {
    match (key, candidate) {
        (LymaKey::String(expected), Value::String(actual)) => {
            actual.to_str().is_ok_and(|actual| actual == *expected)
        }
        (LymaKey::Number(LymaNumber::Integer(expected)), Value::Integer(actual)) => {
            expected == actual
        }
        (LymaKey::Number(LymaNumber::Float(expected)), Value::Number(actual)) => {
            expected.to_bits() == actual.to_bits()
        }
        (LymaKey::Boolean(expected), Value::Boolean(actual)) => expected == actual,
        _ => false,
    }
}

fn lua_runtime_error(error: &LuaRuntimeError) -> omnilua::Error {
    omnilua::Error::from(LuaError::runtime(format_args!(
        "{}",
        error.diagnostic.message
    )))
}

fn function_profile_error(span: Option<lyma_syntax::source::Span>) -> LuaRuntimeError {
    profile_error(
        lyma_syntax::DiagnosticCode::FunctionValueNotAllowedInThisProfile,
        "profile forbids function values",
        span,
    )
}

fn profile_error(
    code: lyma_syntax::DiagnosticCode,
    message: &str,
    span: Option<lyma_syntax::source::Span>,
) -> LuaRuntimeError {
    let mut diagnostic = lyma_syntax::Diagnostic::new(code, lyma_syntax::Severity::Error);
    diagnostic.message = String::from(message);
    diagnostic.primary_span = span;
    LuaRuntimeError::new(
        engine_name(),
        LuaRuntimePhase::Conversion,
        Box::new(diagnostic),
    )
}

fn conversion_error(message: &str, span: Option<lyma_syntax::source::Span>) -> LuaRuntimeError {
    LuaRuntimeError::runtime_error(engine_name(), LuaRuntimePhase::Conversion, message, span)
}

fn runtime_conversion_error(
    error: &omnilua::Error,
    context: &str,
    span: Option<lyma_syntax::source::Span>,
) -> LuaRuntimeError {
    LuaRuntimeError::runtime_error(
        engine_name(),
        LuaRuntimePhase::Conversion,
        format!("{context}: {error}"),
        span,
    )
}

const fn policy_span_none() -> Option<lyma_syntax::source::Span> {
    None
}

fn positive_integral_index(value: f64) -> Option<usize> {
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 {
        return None;
    }
    format!("{value:.0}").parse::<usize>().ok()
}
