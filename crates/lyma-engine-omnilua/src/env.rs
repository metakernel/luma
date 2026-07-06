//! `OmniLua` environment and module adapters.

use lyma_runtime::{LuaRuntimeError, RuntimeEnvironment, RuntimeModule};
use lyma_syntax::LymaValue;
use omnilua::{Lua, Table, Value};

use crate::{
    convert::{ReadOnlyNamespace, freeze_runtime_value, thaw_runtime_value},
    engine::OmniLuaValue,
    engine_name,
};

/// Detached safe module definition for `OmniLua` environments.
#[derive(Debug, Clone)]
pub struct OmniLuaModule {
    pub(crate) name: String,
    pub(crate) exports: Vec<(String, LymaValue)>,
}

impl RuntimeModule for OmniLuaModule {
    type RuntimeValue = OmniLuaValue;

    fn module_name(&self) -> &str {
        &self.name
    }

    fn exports(&self) -> Result<Vec<(String, Self::RuntimeValue)>, LuaRuntimeError> {
        Ok(self
            .exports
            .iter()
            .map(|(name, value)| (name.clone(), OmniLuaValue::Frozen(value.clone())))
            .collect())
    }
}

/// Cloneable execution environment descriptor for `OmniLua`.
#[derive(Debug, Clone, Default)]
pub struct OmniLuaEnvironment {
    builtins: Vec<(String, LymaValue)>,
    context: Vec<(String, LymaValue)>,
    modules: Vec<OmniLuaModule>,
}

impl OmniLuaEnvironment {
    pub(crate) fn materialize(&self) -> Result<Lua, LuaRuntimeError> {
        let lua = Lua::try_new().map_err(|error| {
            LuaRuntimeError::runtime_error(
                engine_name(),
                lyma_runtime::LuaRuntimePhase::Environment,
                format!("failed to create OmniLua state: {error}"),
                None,
            )
        })?;
        sanitize_globals(&lua)?;

        let globals = lua.globals();
        for (name, value) in &self.builtins {
            globals
                .set(name.as_str(), thaw_runtime_value(&lua, value)?)
                .map_err(|error| {
                    environment_error(&error, &format!("failed to inject builtin '{name}'"))
                })?;
        }
        for (name, value) in &self.context {
            globals
                .set(name.as_str(), thaw_runtime_value(&lua, value)?)
                .map_err(|error| {
                    environment_error(&error, &format!("failed to inject context '{name}'"))
                })?;
        }
        for module in &self.modules {
            let namespace = build_snapshot_namespace(&lua, module.module_name(), &module.exports)?;
            globals
                .set(module.module_name(), namespace)
                .map_err(|error| {
                    environment_error(
                        &error,
                        &format!("failed to inject module '{}'", module.module_name()),
                    )
                })?;
        }

        Ok(lua)
    }
}

impl RuntimeEnvironment for OmniLuaEnvironment {
    type RuntimeValue = OmniLuaValue;
    type RuntimeModule = OmniLuaModule;

    fn fork_isolated(&self) -> Result<Self, LuaRuntimeError> {
        Ok(self.clone())
    }

    fn inject_builtin(
        &mut self,
        name: impl Into<String>,
        value: Self::RuntimeValue,
    ) -> Result<(), LuaRuntimeError> {
        self.builtins
            .push((name.into(), freeze_runtime_value(value)?));
        Ok(())
    }

    fn inject_context(
        &mut self,
        name: impl Into<String>,
        value: Self::RuntimeValue,
    ) -> Result<(), LuaRuntimeError> {
        self.context
            .push((name.into(), freeze_runtime_value(value)?));
        Ok(())
    }

    fn inject_module(&mut self, module: Self::RuntimeModule) -> Result<(), LuaRuntimeError> {
        self.modules.push(module);
        Ok(())
    }
}

fn sanitize_globals(lua: &Lua) -> Result<(), LuaRuntimeError> {
    let globals = lua.globals();
    let safe_scalars = capture_globals(
        &globals,
        &[
            "assert", "error", "ipairs", "next", "pairs", "pcall", "select", "tonumber",
            "tostring", "type", "_VERSION",
        ],
    )?;
    let safe_math = capture_namespace_except(&globals, "math", &["random", "randomseed"])?;
    let safe_string = capture_namespace(&globals, "string")?;
    let safe_table = capture_namespace(&globals, "table")?;
    let safe_utf8 = capture_namespace(&globals, "utf8")?;

    for (key, _) in globals
        .raw_pairs()
        .map_err(|error| environment_error(&error, "failed to enumerate globals"))?
    {
        globals
            .set(key, Value::Nil)
            .map_err(|error| environment_error(&error, "failed to clear global"))?;
    }

    for (name, value) in safe_scalars {
        globals.set(name.as_str(), value).map_err(|error| {
            environment_error(&error, &format!("failed to restore safe global '{name}'"))
        })?;
    }

    globals
        .set(
            "null",
            Value::UserData(
                lua.create_userdata(crate::convert::NullSentinel)
                    .map_err(|error| {
                        environment_error(&error, "failed to install null sentinel")
                    })?,
            ),
        )
        .map_err(|error| environment_error(&error, "failed to bind null sentinel"))?;

    install_namespace(lua, &globals, "math", safe_math)?;
    install_namespace(lua, &globals, "string", safe_string)?;
    install_namespace(lua, &globals, "table", safe_table)?;
    if !safe_utf8.is_empty() {
        install_namespace(lua, &globals, "utf8", safe_utf8)?;
    }
    Ok(())
}

fn capture_globals(
    globals: &Table,
    names: &[&str],
) -> Result<Vec<(String, Value)>, LuaRuntimeError> {
    names
        .iter()
        .map(|name| {
            globals
                .get::<_, Value>(*name)
                .map(|value| (String::from(*name), value))
                .map_err(|error| {
                    environment_error(&error, &format!("failed to capture global '{name}'"))
                })
        })
        .collect()
}

fn capture_namespace(globals: &Table, name: &str) -> Result<Vec<(String, Value)>, LuaRuntimeError> {
    capture_namespace_except(globals, name, &[])
}

fn capture_namespace_except(
    globals: &Table,
    name: &str,
    excluded_names: &[&str],
) -> Result<Vec<(String, Value)>, LuaRuntimeError> {
    let table = globals.get::<_, Table>(name).map_err(|error| {
        environment_error(&error, &format!("failed to capture namespace '{name}'"))
    })?;
    table
        .raw_pairs()
        .map_err(|error| {
            environment_error(&error, &format!("failed to enumerate namespace '{name}'"))
        })?
        .into_iter()
        .filter_map(|(key, value)| match key {
            Value::String(key) => Some(
                key.to_str()
                    .map(|key| (key, value))
                    .map_err(|error| {
                        environment_error(
                            &error,
                            &format!("failed to decode namespace key in '{name}'"),
                        )
                    })
                    .map(|(key, value)| {
                        if excluded_names.contains(&key.as_str()) {
                            None
                        } else {
                            Some((key, value))
                        }
                    }),
            ),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| entries.into_iter().flatten().collect())
}

fn install_namespace(
    lua: &Lua,
    globals: &Table,
    name: &str,
    entries: Vec<(String, Value)>,
) -> Result<(), LuaRuntimeError> {
    globals
        .set(
            name,
            Value::UserData(
                lua.create_userdata(ReadOnlyNamespace {
                    label: format!("lyma:{name}"),
                    entries,
                })
                .map_err(|error| {
                    environment_error(&error, &format!("failed to create namespace '{name}'"))
                })?,
            ),
        )
        .map_err(|error| {
            environment_error(&error, &format!("failed to install namespace '{name}'"))
        })
}

fn build_snapshot_namespace(
    lua: &Lua,
    name: &str,
    exports: &[(String, LymaValue)],
) -> Result<Value, LuaRuntimeError> {
    let mut entries = Vec::with_capacity(exports.len());
    for (export_name, export_value) in exports {
        entries.push((export_name.clone(), thaw_runtime_value(lua, export_value)?));
    }
    Ok(Value::UserData(
        lua.create_userdata(ReadOnlyNamespace {
            label: format!("lyma:module:{name}"),
            entries,
        })
        .map_err(|error| {
            environment_error(
                &error,
                &format!("failed to create module namespace '{name}'"),
            )
        })?,
    ))
}

fn environment_error(error: &omnilua::Error, message: &str) -> LuaRuntimeError {
    LuaRuntimeError::runtime_error(
        engine_name(),
        lyma_runtime::LuaRuntimePhase::Environment,
        format!("{message}: {error}"),
        None,
    )
}
