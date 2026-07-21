//! Lua-backed plugins for zoeken.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use md5::{Digest as _, Md5};
use mlua::{
    Function, HookTriggers, Lua, RegistryKey, Table, UserData, UserDataFields, Value, VmState,
};
use sha1::{Digest as _, Sha1};
use sha2::{Sha224, Sha256, Sha384, Sha512};
use thiserror::Error;
use zoeken_query::{Locale, SearchQuery};
use zoeken_results::{Answer, Infobox, MainResult, Result_, Template};

use crate::{Plugin, PluginCtx, PluginInfo, PluginKind, ResultContainerMut};

pub const API_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum LuaPluginError {
    #[error("failed to read Lua plugin `{path}`: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to load Lua plugin `{name}`: {source}")]
    Load { name: String, source: mlua::Error },
    #[error("Lua plugin `{id}` has unsupported api_version {api_version}")]
    UnsupportedApiVersion { id: String, api_version: u32 },
    #[error("Lua plugin `{id}` declares invalid kind `{kind}`")]
    InvalidKind { id: String, kind: String },
    #[error("Lua plugin `{id}` declares unsupported capability `{capability}`")]
    InvalidCapability { id: String, capability: String },
    #[error("Lua plugin `{id}` has invalid metadata: {message}")]
    InvalidMetadata { id: String, message: String },
}

#[derive(Debug, Clone)]
pub struct LuaRuntimeConfig {
    pub vm_pool_size: usize,
    pub hook_timeout: Duration,
    pub memory_limit: usize,
    pub instruction_budget: usize,
    pub allowed_capabilities: BTreeSet<String>,
    pub outbound: Option<Arc<zoeken_network::NetworkManager>>,
}

impl Default for LuaRuntimeConfig {
    fn default() -> Self {
        Self {
            vm_pool_size: 2,
            // Soft wall-clock budget for a single hook. Host callbacks (e.g.
            // clean_url over ~144 ClearURLs rules) run inside the Lua call and
            // can legitimately take tens of ms on Windows.
            hook_timeout: Duration::from_millis(250),
            // Embedded currencies+units alone exceed a few MB once expanded into
            // Lua tables; 8 MiB OOMs legitimate answerer plugins at init/hook time.
            memory_limit: 32 * 1024 * 1024,
            instruction_budget: 1_000_000,
            allowed_capabilities: [
                "query", "result", "results", "answers", "request", "data", "log", "utils",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            outbound: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct LuaPluginMetrics {
    pub hook_failures: AtomicUsize,
    pub load_failures: AtomicUsize,
    pub init_failures: AtomicUsize,
    pub timeouts: AtomicUsize,
    pub dropped_results: AtomicUsize,
    pub appended_results: AtomicUsize,
}

pub struct LuaPlugin {
    info: PluginInfo,
    pool: Vec<Mutex<LuaVm>>,
    cursor: AtomicUsize,
    metrics: Arc<LuaPluginMetrics>,
    timeout: Duration,
}

struct LuaVm {
    lua: Lua,
    module: RegistryKey,
    disabled: bool,
    instruction_counter: Arc<AtomicUsize>,
    deadline: Arc<Mutex<Option<Instant>>>,
    /// Full `ctx.data` snapshot — built once; re-marshalling every hook OOMs/timeouts.
    cached_data: Option<RegistryKey>,
    /// Host utils table — built once (closes over tracker patterns).
    cached_utils: Option<RegistryKey>,
}

impl LuaPlugin {
    pub fn from_source(
        name: impl Into<String>,
        source: impl Into<String>,
        data: Arc<zoeken_data::DataBundle>,
        config: LuaRuntimeConfig,
    ) -> Result<Self, LuaPluginError> {
        let name = name.into();
        let source = source.into();
        let first = build_lua(config.memory_limit, config.instruction_budget)?;
        let first_lua = first.lua;
        let first_module = load_module(&first_lua, &name, &source)?;
        let info = plugin_info(&first_module, &config)?;
        let metrics = Arc::new(LuaPluginMetrics::default());
        let (first_data, first_utils) =
            build_static_ctx_caches(&first_lua, &info, data.as_ref(), &config)?;

        let mut pool = Vec::with_capacity(config.vm_pool_size.max(1));
        let first_disabled = run_init(
            &first_lua,
            &first_module,
            &info,
            first_data.as_ref(),
            first_utils.as_ref(),
            &metrics,
        );
        let first_key = first_lua
            .create_registry_value(first_module)
            .map_err(|source| LuaPluginError::Load {
                name: name.clone(),
                source,
            })?;
        pool.push(Mutex::new(LuaVm {
            lua: first_lua,
            module: first_key,
            disabled: first_disabled,
            instruction_counter: first.instruction_counter,
            deadline: first.deadline,
            cached_data: first_data,
            cached_utils: first_utils,
        }));

        for _ in 1..config.vm_pool_size.max(1) {
            let built = build_lua(config.memory_limit, config.instruction_budget)?;
            let lua = built.lua;
            let module = load_module(&lua, &name, &source)?;
            let (cached_data, cached_utils) =
                build_static_ctx_caches(&lua, &info, data.as_ref(), &config)?;
            let disabled = run_init(
                &lua,
                &module,
                &info,
                cached_data.as_ref(),
                cached_utils.as_ref(),
                &metrics,
            );
            let key = lua
                .create_registry_value(module)
                .map_err(|source| LuaPluginError::Load {
                    name: name.clone(),
                    source,
                })?;
            pool.push(Mutex::new(LuaVm {
                lua,
                module: key,
                disabled,
                instruction_counter: built.instruction_counter,
                deadline: built.deadline,
                cached_data,
                cached_utils,
            }));
        }

        Ok(Self {
            info,
            pool,
            cursor: AtomicUsize::new(0),
            metrics,
            timeout: config.hook_timeout,
        })
    }

    pub fn from_file(
        path: &Path,
        data: Arc<zoeken_data::DataBundle>,
        config: LuaRuntimeConfig,
    ) -> Result<Self, LuaPluginError> {
        let source = std::fs::read_to_string(path).map_err(|source| LuaPluginError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin")
            .to_string();
        Self::from_source(name, source, data, config)
    }

    pub fn metrics(&self) -> Arc<LuaPluginMetrics> {
        Arc::clone(&self.metrics)
    }

    fn with_module<R>(
        &self,
        hook: &str,
        default: R,
        f: impl FnOnce(&Lua, Table, Option<Table>, Option<Table>) -> mlua::Result<R>,
    ) -> R {
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % self.pool.len();
        let Ok(vm) = self.pool[idx].lock() else {
            self.metrics.hook_failures.fetch_add(1, Ordering::Relaxed);
            return default;
        };
        if vm.disabled {
            return default;
        }
        vm.instruction_counter.store(0, Ordering::Relaxed);
        if let Ok(mut deadline) = vm.deadline.lock() {
            *deadline = Some(Instant::now() + self.timeout);
        }
        let started = Instant::now();
        let module = match vm.lua.registry_value::<Table>(&vm.module) {
            Ok(module) => module,
            Err(error) => {
                tracing::warn!(plugin = self.info.id, hook, %error, "Lua plugin registry lookup failed");
                self.metrics.hook_failures.fetch_add(1, Ordering::Relaxed);
                return default;
            }
        };
        let cached_data = match vm.cached_data.as_ref() {
            Some(key) => match vm.lua.registry_value::<Table>(key) {
                Ok(table) => Some(table),
                Err(error) => {
                    tracing::warn!(plugin = self.info.id, hook, %error, "Lua cached data lookup failed");
                    self.metrics.hook_failures.fetch_add(1, Ordering::Relaxed);
                    return default;
                }
            },
            None => None,
        };
        let cached_utils = match vm.cached_utils.as_ref() {
            Some(key) => match vm.lua.registry_value::<Table>(key) {
                Ok(table) => Some(table),
                Err(error) => {
                    tracing::warn!(plugin = self.info.id, hook, %error, "Lua cached utils lookup failed");
                    self.metrics.hook_failures.fetch_add(1, Ordering::Relaxed);
                    return default;
                }
            },
            None => None,
        };
        let result = f(&vm.lua, module, cached_data, cached_utils);
        if let Ok(mut deadline) = vm.deadline.lock() {
            *deadline = None;
        }
        let elapsed = started.elapsed();
        match result {
            Ok(value) => {
                // Soft budget only: deadline already aborts runaway Lua. Log at
                // debug so host-side work (clean_url, result clone) does not spam.
                if elapsed > self.timeout {
                    tracing::debug!(
                        plugin = self.info.id,
                        hook,
                        elapsed_ms = elapsed.as_millis() as u64,
                        "Lua plugin hook exceeded soft wall-clock budget"
                    );
                }
                value
            }
            Err(error) => {
                let timed_out = error.to_string().contains("timeout");
                if timed_out {
                    self.metrics.timeouts.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        plugin = self.info.id,
                        hook,
                        elapsed_ms = elapsed.as_millis() as u64,
                        "Lua plugin hook exceeded wall-clock budget"
                    );
                } else {
                    self.metrics.hook_failures.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(plugin = self.info.id, hook, %error, "Lua plugin hook failed");
                }
                default
            }
        }
    }
}

impl Plugin for LuaPlugin {
    fn id(&self) -> &str {
        &self.info.id
    }

    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    fn metrics_snapshot(&self) -> Option<crate::PluginMetricsSnapshot> {
        let m = &self.metrics;
        Some(crate::PluginMetricsSnapshot {
            id: self.info.id.clone(),
            hook_failures: m.hook_failures.load(Ordering::Relaxed),
            load_failures: m.load_failures.load(Ordering::Relaxed),
            init_failures: m.init_failures.load(Ordering::Relaxed),
            timeouts: m.timeouts.load(Ordering::Relaxed),
            dropped_results: m.dropped_results.load(Ordering::Relaxed),
            appended_results: m.appended_results.load(Ordering::Relaxed),
        })
    }

    fn on_pre_search(&self, query: &mut SearchQuery, ctx: &PluginCtx) -> bool {
        self.with_module(
            "pre_search",
            true,
            |lua, module, cached_data, cached_utils| {
                let Some(func) = optional_function(&module, "pre_search")? else {
                    return Ok(true);
                };
                let table = query_to_table(lua, query, true)?;
                let ctx = ctx_to_table(
                    lua,
                    ctx,
                    &self.info,
                    cached_data.as_ref(),
                    cached_utils.as_ref(),
                )?;
                let proceed = func
                    .call::<Option<bool>>((table.clone(), ctx))?
                    .unwrap_or(true);
                apply_query_table(&table, query)?;
                Ok(proceed)
            },
        )
    }

    fn on_pre_search_answers(&self, query: &SearchQuery, ctx: &PluginCtx) -> Vec<Answer> {
        self.with_module(
            "pre_search_answers",
            Vec::new(),
            |lua, module, cached_data, cached_utils| {
                let func = match optional_function(&module, "pre_search_answers")? {
                    Some(func) => func,
                    None => optional_function(&module, "answer")?.ok_or_else(|| {
                        mlua::Error::external("missing pre_search_answers/answer hook")
                    })?,
                };
                let query = query_to_table(lua, query, false)?;
                let ctx = ctx_to_table(
                    lua,
                    ctx,
                    &self.info,
                    cached_data.as_ref(),
                    cached_utils.as_ref(),
                )?;
                let value: Value = func.call((query, ctx))?;
                answers_from_value(value)
            },
        )
    }

    fn on_result(&self, result: &mut Result_, query: &SearchQuery, ctx: &PluginCtx) -> bool {
        let keep = self.with_module(
            "on_result",
            true,
            |lua, module, cached_data, cached_utils| {
                let Some(func) = optional_function(&module, "on_result")? else {
                    return Ok(true);
                };
                let handle = ResultHandle::new(result.clone());
                let userdata = lua.create_userdata(handle.clone())?;
                let query = query_to_table(lua, query, false)?;
                let ctx = ctx_to_table(
                    lua,
                    ctx,
                    &self.info,
                    cached_data.as_ref(),
                    cached_utils.as_ref(),
                )?;
                let hook_result = func
                    .call::<Option<bool>>((userdata, query, ctx))
                    .and_then(|keep| handle.take().map(|result| (keep.unwrap_or(true), result)));
                if hook_result.is_err() {
                    let _ = handle.expire();
                }
                let (keep, updated) = hook_result?;
                *result = updated;
                Ok(keep)
            },
        );
        if !keep {
            self.metrics.dropped_results.fetch_add(1, Ordering::Relaxed);
        }
        keep
    }

    fn on_results(
        &self,
        container: &mut dyn ResultContainerMut,
        query: &SearchQuery,
        ctx: &PluginCtx,
    ) {
        let before = container.main_results_mut().len();
        self.with_module(
            "on_results",
            (),
            |lua, module, cached_data, cached_utils| {
                let Some(func) = optional_function(&module, "on_results")? else {
                    return Ok(());
                };
                let (table, handles) = container_to_table(lua, container)?;
                let query = query_to_table(lua, query, false)?;
                let ctx = ctx_to_table(
                    lua,
                    ctx,
                    &self.info,
                    cached_data.as_ref(),
                    cached_utils.as_ref(),
                )?;
                let result = func
                    .call::<()>((table.clone(), query, ctx))
                    .and_then(|()| apply_container_table(&table, container));
                expire_handles(&handles);
                result
            },
        );
        let after = container.main_results_mut().len();
        if after > before {
            self.metrics
                .appended_results
                .fetch_add(after - before, Ordering::Relaxed);
        } else {
            self.metrics
                .dropped_results
                .fetch_add(before - after, Ordering::Relaxed);
        }
    }

    fn on_post_search(&self, container: &mut dyn ResultContainerMut, ctx: &PluginCtx) {
        let before = container.main_results_mut().len();
        self.with_module(
            "post_search",
            (),
            |lua, module, cached_data, cached_utils| {
                let Some(func) = optional_function(&module, "post_search")? else {
                    return Ok(());
                };
                let (table, handles) = container_to_table(lua, container)?;
                let ctx = ctx_to_table(
                    lua,
                    ctx,
                    &self.info,
                    cached_data.as_ref(),
                    cached_utils.as_ref(),
                )?;
                let result = func
                    .call::<()>((table.clone(), ctx))
                    .and_then(|()| apply_container_table(&table, container));
                expire_handles(&handles);
                result
            },
        );
        let after = container.main_results_mut().len();
        if after > before {
            self.metrics
                .appended_results
                .fetch_add(after - before, Ordering::Relaxed);
        } else {
            self.metrics
                .dropped_results
                .fetch_add(before - after, Ordering::Relaxed);
        }
    }
}

pub fn load_plugins_from_dir(
    dir: &Path,
    data: Arc<zoeken_data::DataBundle>,
    config: LuaRuntimeConfig,
) -> Vec<Arc<dyn Plugin>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("lua"))
        .collect();
    paths.sort();

    let mut plugins: Vec<Arc<dyn Plugin>> = paths
        .into_iter()
        .filter_map(
            |path| match LuaPlugin::from_file(&path, Arc::clone(&data), config.clone()) {
                Ok(plugin) => Some(Arc::new(plugin) as Arc<dyn Plugin>),
                Err(error) => {
                    tracing::warn!(%error, "skipping Lua plugin");
                    None
                }
            },
        )
        .collect();
    crate::sort_plugins(&mut plugins, &[]).unwrap_or_else(|error| {
        tracing::warn!(%error, "using fallback Lua plugin order");
    });
    plugins
}

struct BuiltLua {
    lua: Lua,
    instruction_counter: Arc<AtomicUsize>,
    deadline: Arc<Mutex<Option<Instant>>>,
}

fn build_lua(memory_limit: usize, instruction_budget: usize) -> Result<BuiltLua, LuaPluginError> {
    let lua = Lua::new();
    let globals = lua.globals();
    for name in [
        "io", "os", "package", "require", "dofile", "loadfile", "load", "debug",
    ] {
        globals
            .set(name, Value::Nil)
            .map_err(|source| LuaPluginError::Load {
                name: "sandbox".to_string(),
                source,
            })?;
    }
    lua.set_memory_limit(memory_limit)
        .map_err(|source| LuaPluginError::Load {
            name: "runtime".to_string(),
            source,
        })?;
    let instruction_counter = Arc::new(AtomicUsize::new(0));
    let hook_counter = Arc::clone(&instruction_counter);
    let deadline = Arc::new(Mutex::new(None));
    let hook_deadline = Arc::clone(&deadline);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(1000),
        move |_, _| {
            if hook_counter.fetch_add(1000, Ordering::Relaxed) > instruction_budget {
                return Err(mlua::Error::external("Lua instruction budget exceeded"));
            }
            if let Ok(deadline) = hook_deadline.lock()
                && let Some(deadline) = *deadline
                && Instant::now() > deadline
            {
                return Err(mlua::Error::external("Lua hook timeout exceeded"));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|source| LuaPluginError::Load {
        name: "sandbox".to_string(),
        source,
    })?;
    Ok(BuiltLua {
        lua,
        instruction_counter,
        deadline,
    })
}

fn load_module(lua: &Lua, name: &str, source: &str) -> Result<Table, LuaPluginError> {
    lua.load(source)
        .set_name(name)
        .eval::<Table>()
        .map_err(|source| LuaPluginError::Load {
            name: name.to_string(),
            source,
        })
}

fn run_init(
    lua: &Lua,
    module: &Table,
    info: &PluginInfo,
    cached_data: Option<&RegistryKey>,
    cached_utils: Option<&RegistryKey>,
    metrics: &LuaPluginMetrics,
) -> bool {
    let Ok(Some(func)) = optional_function(module, "init") else {
        return false;
    };
    let data = match cached_data {
        Some(key) => match lua.registry_value::<Table>(key) {
            Ok(table) => Some(table),
            Err(error) => {
                tracing::warn!(plugin = info.id, %error, "failed to load cached Lua data for init");
                metrics.init_failures.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        },
        None => None,
    };
    let utils = match cached_utils {
        Some(key) => match lua.registry_value::<Table>(key) {
            Ok(table) => Some(table),
            Err(error) => {
                tracing::warn!(plugin = info.id, %error, "failed to load cached Lua utils for init");
                metrics.init_failures.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        },
        None => None,
    };
    let ctx = match init_ctx_to_table(lua, data.as_ref(), utils.as_ref()) {
        Ok(ctx) => ctx,
        Err(error) => {
            tracing::warn!(plugin = info.id, %error, "failed to build Lua init context");
            metrics.init_failures.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    };
    match func.call::<Option<bool>>(ctx) {
        Ok(disabled) => disabled == Some(false),
        Err(error) => {
            tracing::warn!(plugin = info.id, %error, "Lua plugin init failed");
            metrics.init_failures.fetch_add(1, Ordering::Relaxed);
            true
        }
    }
}

fn build_static_ctx_caches(
    lua: &Lua,
    info: &PluginInfo,
    data: &zoeken_data::DataBundle,
    config: &LuaRuntimeConfig,
) -> Result<(Option<RegistryKey>, Option<RegistryKey>), LuaPluginError> {
    let cached_data = if has_capability(info, "data") {
        let table = data_to_table(lua, data).map_err(|source| LuaPluginError::Load {
            name: info.id.clone(),
            source,
        })?;
        Some(
            lua.create_registry_value(table)
                .map_err(|source| LuaPluginError::Load {
                    name: info.id.clone(),
                    source,
                })?,
        )
    } else {
        None
    };
    let cached_utils = if has_capability(info, "utils") {
        let table = utils_to_table(lua, &data.tracker_patterns, config.outbound.clone()).map_err(
            |source| LuaPluginError::Load {
                name: info.id.clone(),
                source,
            },
        )?;
        Some(
            lua.create_registry_value(table)
                .map_err(|source| LuaPluginError::Load {
                    name: info.id.clone(),
                    source,
                })?,
        )
    } else {
        None
    };
    Ok((cached_data, cached_utils))
}

fn plugin_info(module: &Table, config: &LuaRuntimeConfig) -> Result<PluginInfo, LuaPluginError> {
    let id = metadata_string(module, "id", "lua_plugin")?;
    let api_version = module
        .get::<Option<u32>>("api_version")
        .map_err(|source| LuaPluginError::InvalidMetadata {
            id: id.clone(),
            message: format!("api_version must be an integer: {source}"),
        })?
        .unwrap_or(1);
    if api_version != API_VERSION {
        return Err(LuaPluginError::UnsupportedApiVersion { id, api_version });
    }
    let kind_raw = metadata_string(module, "kind", "result_plugin")?;
    let kind = match kind_raw.as_str() {
        "result_plugin" => PluginKind::ResultPlugin,
        "answerer" => PluginKind::Answerer,
        "both" => PluginKind::Both,
        _ => {
            return Err(LuaPluginError::InvalidKind { id, kind: kind_raw });
        }
    };
    let capabilities = metadata_string_vec(module, "capabilities", &id)?;
    for capability in &capabilities {
        if !config.allowed_capabilities.contains(capability) {
            return Err(LuaPluginError::InvalidCapability {
                id,
                capability: capability.clone(),
            });
        }
    }
    let default_enabled = module
        .get::<Option<bool>>("default_enabled")
        .map_err(|source| LuaPluginError::InvalidMetadata {
            id: id.clone(),
            message: format!("default_enabled must be a boolean: {source}"),
        })?
        .unwrap_or(true);
    let order = module
        .get::<Option<i32>>("order")
        .map_err(|source| LuaPluginError::InvalidMetadata {
            id: id.clone(),
            message: format!("order must be an integer: {source}"),
        })?
        .unwrap_or(0);
    Ok(PluginInfo {
        name: metadata_string(module, "name", &id)?,
        description: metadata_string(module, "description", "")?,
        examples: metadata_string_vec(module, "examples", &id)?,
        version: metadata_string(module, "version", "")?,
        api_version,
        kind,
        default_enabled,
        keywords: metadata_string_vec(module, "keywords", &id)?,
        preference_section: metadata_string(module, "preference_section", "plugins")?,
        order,
        after: metadata_string_vec(module, "after", &id)?,
        before: metadata_string_vec(module, "before", &id)?,
        capabilities,
        id,
    })
}

fn optional_function(table: &Table, name: &str) -> mlua::Result<Option<Function>> {
    match table.get::<Value>(name)? {
        Value::Function(func) => Ok(Some(func)),
        Value::Nil => Ok(None),
        _ => Err(mlua::Error::external(format!("{name} is not a function"))),
    }
}

fn get_string(table: &Table, key: &str) -> Option<String> {
    table.get::<Option<String>>(key).ok().flatten()
}

fn metadata_string(table: &Table, key: &str, default: &str) -> Result<String, LuaPluginError> {
    let id = get_string(table, "id").unwrap_or_else(|| "lua_plugin".to_string());
    match table.get::<Value>(key) {
        Ok(Value::Nil) => Ok(default.to_string()),
        Ok(Value::String(value)) => value.to_str().map(|s| s.to_string()).map_err(|source| {
            LuaPluginError::InvalidMetadata {
                id,
                message: format!("{key} must be UTF-8: {source}"),
            }
        }),
        Ok(_) => Err(LuaPluginError::InvalidMetadata {
            id,
            message: format!("{key} must be a string"),
        }),
        Err(source) => Err(LuaPluginError::InvalidMetadata {
            id,
            message: format!("failed to read {key}: {source}"),
        }),
    }
}

fn metadata_string_vec(table: &Table, key: &str, id: &str) -> Result<Vec<String>, LuaPluginError> {
    match table.get::<Value>(key) {
        Ok(Value::Nil) => Ok(Vec::new()),
        Ok(Value::String(value)) => value
            .to_str()
            .map(|s| vec![s.to_string()])
            .map_err(|source| LuaPluginError::InvalidMetadata {
                id: id.to_string(),
                message: format!("{key} must contain UTF-8 strings: {source}"),
            }),
        Ok(Value::Table(values)) => {
            let mut out = Vec::new();
            for value in values.sequence_values::<Value>() {
                match value {
                    Ok(Value::String(value)) => {
                        out.push(value.to_str().map(|s| s.to_string()).map_err(|source| {
                            LuaPluginError::InvalidMetadata {
                                id: id.to_string(),
                                message: format!("{key} must contain UTF-8 strings: {source}"),
                            }
                        })?);
                    }
                    Ok(_) => {
                        return Err(LuaPluginError::InvalidMetadata {
                            id: id.to_string(),
                            message: format!("{key} must be a string list"),
                        });
                    }
                    Err(source) => {
                        return Err(LuaPluginError::InvalidMetadata {
                            id: id.to_string(),
                            message: format!("{key} must be a string list: {source}"),
                        });
                    }
                }
            }
            Ok(out)
        }
        Ok(_) => Err(LuaPluginError::InvalidMetadata {
            id: id.to_string(),
            message: format!("{key} must be a string or string list"),
        }),
        Err(source) => Err(LuaPluginError::InvalidMetadata {
            id: id.to_string(),
            message: format!("failed to read {key}: {source}"),
        }),
    }
}

fn init_ctx_to_table(
    lua: &Lua,
    cached_data: Option<&Table>,
    cached_utils: Option<&Table>,
) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;
    if let Some(data) = cached_data {
        ctx.set("data", data)?;
    }
    if let Some(utils) = cached_utils {
        ctx.set("utils", utils)?;
    }
    Ok(ctx)
}

fn ctx_to_table(
    lua: &Lua,
    ctx: &PluginCtx,
    info: &PluginInfo,
    cached_data: Option<&Table>,
    cached_utils: Option<&Table>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if has_capability(info, "request") {
        table.set("client_ip", ctx.client_ip.clone())?;
        table.set("user_agent", ctx.user_agent.clone())?;
        table.set("method", ctx.method.clone())?;
        table.set("locale", ctx.locale.clone())?;
        table.set("image_proxy", ctx.image_proxy)?;
        let headers = lua.create_table()?;
        for (name, value) in &ctx.headers {
            headers.set(name.as_str(), value.as_str())?;
        }
        table.set("headers", headers)?;
    }
    if let Some(data) = cached_data {
        table.set("data", data)?;
    }
    if has_capability(info, "log") {
        let log = lua.create_table()?;
        log.set(
            "warn",
            lua.create_function(|_, msg: String| {
                tracing::warn!(message = msg, "Lua plugin warning");
                Ok(())
            })?,
        )?;
        table.set("log", log)?;
    }
    if let Some(utils) = cached_utils {
        table.set("utils", utils)?;
    }
    Ok(table)
}

fn has_capability(info: &PluginInfo, capability: &str) -> bool {
    info.capabilities
        .iter()
        .any(|declared| declared == capability)
}

fn data_to_table(lua: &Lua, data: &zoeken_data::DataBundle) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("currency_count", data.currencies.iso_len())?;
    table.set("locale_count", data.locales.locale_names.len())?;
    table.set("unit_count", data.units.len())?;
    table.set("tracker_pattern_count", data.tracker_patterns.rule_count())?;
    table.set("ahmia_blacklist_count", data.ahmia_blacklist.len())?;
    table.set("currencies", currencies_to_table(lua, &data.currencies)?)?;
    table.set("units", units_to_table(lua, &data.units)?)?;
    table.set(
        "tracker_patterns",
        tracker_patterns_to_table(lua, &data.tracker_patterns)?,
    )?;
    table.set(
        "ahmia_blacklist",
        ahmia_blacklist_to_table(lua, &data.ahmia_blacklist)?,
    )?;
    table.set("doi_resolver", data.plugin_data.doi_resolver.clone())?;
    table.set("using_tor_proxy", data.plugin_data.using_tor_proxy)?;
    table.set(
        "hostnames",
        hostnames_to_table(lua, &data.plugin_data.hostnames)?,
    )?;
    Ok(table)
}

fn currencies_to_table(lua: &Lua, currencies: &zoeken_data::CurrencyTable) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let iso = lua.create_table()?;
    for (code, langs) in currencies.iter_iso() {
        let names_tbl = lua.create_table()?;
        for (lang, name) in langs {
            names_tbl.set(lang, name)?;
        }
        iso.set(code, names_tbl)?;
    }
    table.set("iso4217", iso)?;
    let names = lua.create_table()?;
    for (name, codes) in currencies.iter_names() {
        let codes_tbl = lua.create_table()?;
        for (idx, code) in codes.enumerate() {
            codes_tbl.set(idx + 1, code)?;
        }
        names.set(name, codes_tbl)?;
    }
    table.set("names", names)?;
    Ok(table)
}

fn units_to_table(lua: &Lua, units: &zoeken_data::UnitTable) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (_id, entry) in units.iter() {
        let (Some(si_name), Some(to_si_factor)) = (entry.si_name, entry.to_si_factor) else {
            continue;
        };
        if entry.symbol.is_empty() {
            continue;
        }
        let item = lua.create_table()?;
        item.set("si_name", si_name)?;
        item.set("symbol", entry.symbol)?;
        item.set("to_si_factor", to_si_factor)?;
        table.push(item)?;
    }
    Ok(table)
}

fn hostnames_to_table(lua: &Lua, rules: &zoeken_data::HostnamesRules) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let replace = lua.create_table()?;
    for (pattern, replacement) in &rules.replace {
        let item = lua.create_table()?;
        item.set("pattern", pattern.as_str())?;
        item.set("replacement", replacement.as_str())?;
        replace.push(item)?;
    }
    table.set("replace", replace)?;
    table.set("remove", string_vec_to_table(lua, &rules.remove)?)?;
    table.set(
        "high_priority",
        string_vec_to_table(lua, &rules.high_priority)?,
    )?;
    table.set(
        "low_priority",
        string_vec_to_table(lua, &rules.low_priority)?,
    )?;
    Ok(table)
}

fn tracker_patterns_to_table(
    lua: &Lua,
    patterns: &zoeken_data::TrackerPatterns,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    // Materialize display rules if the embedded static path left `rules` empty.
    let mut owned = patterns.clone();
    owned.materialize_rules_for_display();
    for rule in &owned.rules {
        let item = lua.create_table()?;
        item.set("url", rule.url_pattern.as_str())?;
        item.set("exceptions", string_vec_to_table(lua, &rule.exceptions)?)?;
        item.set("rules", string_vec_to_table(lua, &rule.rules)?)?;
        table.push(item)?;
    }
    Ok(table)
}

fn utils_to_table(
    lua: &Lua,
    patterns: &zoeken_data::TrackerPatterns,
    outbound: Option<Arc<zoeken_network::NetworkManager>>,
) -> mlua::Result<Table> {
    let utils = lua.create_table()?;
    utils.set(
        "eval",
        lua.create_function(|_, expr: String| Ok(eval_expr(&expr)))?,
    )?;
    utils.set(
        "hash",
        lua.create_function(|_, (algorithm, input): (String, String)| {
            hash_digest(&algorithm, &input).map_err(mlua::Error::external)
        })?,
    )?;
    utils.set("now_utc", lua.create_function(|_, ()| Ok(now_utc()))?)?;
    utils.set(
        "url_host",
        lua.create_function(|_, raw: String| Ok(url_host(&raw)))?,
    )?;
    let patterns = patterns.clone();
    utils.set(
        "clean_url",
        lua.create_function(move |_, raw: String| Ok(patterns.clean_url(&raw)))?,
    )?;
    utils.set(
        "rewrite_host",
        lua.create_function(|_, (raw, pattern, replacement): (String, String, String)| {
            rewrite_host(&raw, &pattern, &replacement).map_err(mlua::Error::external)
        })?,
    )?;
    utils.set(
        "regex_match",
        lua.create_function(|_, (text, pattern): (String, String)| {
            Ok(regex::Regex::new(&pattern)
                .map_err(|error| anyhow!(error))?
                .is_match(&text))
        })?,
    )?;
    utils.set(
        "md5",
        lua.create_function(|_, input: String| {
            hash_digest("md5", &input).map_err(mlua::Error::external)
        })?,
    )?;
    utils.set(
        "extract_doi",
        lua.create_function(|_, raw: String| Ok(extract_doi(&raw)))?,
    )?;
    utils.set(
        "normalize_ip",
        lua.create_function(|_, raw: String| Ok(normalize_ip(&raw)))?,
    )?;
    utils.set(
        "tor_exit_nodes",
        lua.create_function(move |lua, ()| {
            let network = outbound
                .clone()
                .ok_or_else(|| mlua::Error::external("outbound coordination unavailable"))?;
            let nodes = download_tor_exit_nodes(network).map_err(mlua::Error::external)?;
            string_vec_to_table(lua, &nodes)
        })?,
    )?;
    Ok(utils)
}

fn query_to_table(lua: &Lua, query: &SearchQuery, mutable: bool) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("query", query.query.clone())?;
    table.set("q", query.query.clone())?;
    table.set("pageno", query.pageno)?;
    table.set("locale", query.locale.as_str())?;
    table.set("mutable", mutable)?;
    table.set("categories", string_vec_to_table(lua, &query.categories)?)?;
    table.set("engines", string_vec_to_table(lua, &query.engines)?)?;
    Ok(table)
}

fn apply_query_table(table: &Table, query: &mut SearchQuery) -> mlua::Result<()> {
    if let Some(q) = table.get::<Option<String>>("query")? {
        query.query = q;
    } else if let Some(q) = table.get::<Option<String>>("q")? {
        query.query = q;
    }
    if let Some(pageno) = table.get::<Option<u32>>("pageno")? {
        query.pageno = pageno.max(1);
    }
    if let Some(locale) = table.get::<Option<String>>("locale")? {
        query.locale = Locale::new(locale);
    }
    Ok(())
}

#[derive(Clone)]
struct ResultHandle {
    state: Arc<Mutex<ResultHandleState>>,
}

struct ResultHandleState {
    result: Result_,
    live: bool,
}

impl ResultHandle {
    fn new(result: Result_) -> Self {
        Self {
            state: Arc::new(Mutex::new(ResultHandleState { result, live: true })),
        }
    }

    fn snapshot(&self) -> mlua::Result<Result_> {
        self.with_result(Clone::clone)
    }

    fn take(&self) -> mlua::Result<Result_> {
        let mut state = self.lock_live()?;
        state.live = false;
        Ok(state.result.clone())
    }

    fn expire(&self) -> mlua::Result<()> {
        self.lock_live()?.live = false;
        Ok(())
    }

    fn with_result<T>(&self, f: impl FnOnce(&Result_) -> T) -> mlua::Result<T> {
        Ok(f(&self.lock_live()?.result))
    }

    fn with_result_mut<T>(
        &self,
        f: impl FnOnce(&mut Result_) -> mlua::Result<T>,
    ) -> mlua::Result<T> {
        f(&mut self.lock_live()?.result)
    }

    fn lock_live(&self) -> mlua::Result<std::sync::MutexGuard<'_, ResultHandleState>> {
        let state = self
            .state
            .lock()
            .map_err(|_| mlua::Error::external("result handle lock poisoned"))?;
        if !state.live {
            return Err(mlua::Error::external("stale result handle"));
        }
        Ok(state)
    }
}

impl UserData for ResultHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("kind", |_, this| this.with_result(result_kind));
        fields.add_field_method_get("url", |_, this| this.with_result(result_url));
        fields.add_field_method_set("url", |_, this, value: String| {
            validate_url(&value)?;
            this.with_result_mut(|result| {
                set_result_url(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("normalized_url", |_, this| {
            this.with_result(result_normalized_url)
        });
        fields.add_field_method_set("normalized_url", |_, _, _: Value| {
            Err(mlua::Error::external("normalized_url is read-only"))
        });
        fields.add_field_method_get("title", |_, this| this.with_result(result_title));
        fields.add_field_method_set("title", |_, this, value: String| {
            this.with_result_mut(|result| {
                set_result_title(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("content", |_, this| this.with_result(result_content));
        fields.add_field_method_set("content", |_, this, value: String| {
            this.with_result_mut(|result| {
                set_result_content(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("engine", |_, this| this.with_result(result_engine));
        fields.add_field_method_get("score", |_, this| this.with_result(result_score));
        fields.add_field_method_set("score", |_, this, value: f64| {
            if !value.is_finite() {
                return Err(mlua::Error::external("score must be finite"));
            }
            this.with_result_mut(|result| {
                set_result_score(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("priority", |_, this| this.with_result(result_priority));
        fields.add_field_method_set("priority", |_, this, value: String| {
            if !matches!(value.as_str(), "" | "high" | "low") {
                return Err(mlua::Error::external(
                    "priority must be '', 'high', or 'low'",
                ));
            }
            this.with_result_mut(|result| {
                set_result_priority(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("img_src", |_, this| this.with_result(result_img_src));
        fields.add_field_method_set("img_src", |_, this, value: String| {
            validate_optional_url(&value)?;
            this.with_result_mut(|result| {
                set_result_img_src(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("thumbnail_src", |_, this| {
            this.with_result(result_thumbnail_src)
        });
        fields.add_field_method_set("thumbnail_src", |_, this, value: String| {
            validate_optional_url(&value)?;
            this.with_result_mut(|result| {
                set_result_thumbnail_src(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("thumbnail", |_, this| {
            this.with_result(result_thumbnail_src)
        });
        fields.add_field_method_set("thumbnail", |_, this, value: String| {
            validate_optional_url(&value)?;
            this.with_result_mut(|result| {
                set_result_thumbnail_src(result, value);
                Ok(())
            })
        });
        fields.add_field_method_get("doi", |_, this| this.with_result(result_doi));
        fields.add_field_method_set("doi", |_, this, value: String| {
            this.with_result_mut(|result| {
                if let Result_::Paper(r) = result {
                    r.doi = value;
                }
                Ok(())
            })
        });
    }
}

fn result_kind(result: &Result_) -> &'static str {
    match result {
        Result_::Main(_) => "main",
        Result_::Answer(_) => "answer",
        Result_::Image(_) => "image",
        Result_::Paper(_) => "paper",
        Result_::Code(_) => "code",
        Result_::File(_) => "file",
        Result_::KeyValue(_) => "keyvalue",
        Result_::Suggestion(_) => "suggestion",
        Result_::Correction(_) => "correction",
        Result_::Infobox(_) => "infobox",
    }
}

fn result_url(result: &Result_) -> Option<String> {
    match result {
        Result_::Main(r) => Some(r.url.clone()),
        Result_::Image(r) => Some(r.url.clone()),
        Result_::Paper(r) => Some(r.url.clone()),
        Result_::Code(r) => Some(r.url.clone()),
        Result_::File(r) => Some(r.url.clone()),
        Result_::KeyValue(r) => Some(r.url.clone()),
        Result_::Answer(r) => r.url.clone(),
        Result_::Correction(r) => r.url.clone(),
        Result_::Suggestion(_) | Result_::Infobox(_) => None,
    }
}

fn set_result_url(result: &mut Result_, value: String) {
    match result {
        Result_::Main(r) => r.url = value,
        Result_::Image(r) => r.url = value,
        Result_::Paper(r) => r.url = value,
        Result_::Code(r) => r.url = value,
        Result_::File(r) => r.url = value,
        Result_::KeyValue(r) => r.url = value,
        Result_::Answer(r) => r.url = Some(value),
        Result_::Correction(r) => r.url = Some(value),
        Result_::Suggestion(_) | Result_::Infobox(_) => {}
    }
}

fn result_normalized_url(result: &Result_) -> Option<String> {
    match result {
        Result_::Main(r) => Some(r.normalized_url.clone()),
        Result_::Image(r) => Some(r.normalized_url.clone()),
        Result_::Paper(r) => Some(r.normalized_url.clone()),
        Result_::Code(r) => Some(r.normalized_url.clone()),
        Result_::File(r) => Some(r.normalized_url.clone()),
        Result_::KeyValue(r) => Some(r.normalized_url.clone()),
        Result_::Answer(_)
        | Result_::Suggestion(_)
        | Result_::Correction(_)
        | Result_::Infobox(_) => None,
    }
}

fn result_title(result: &Result_) -> Option<String> {
    match result {
        Result_::Main(r) => Some(r.title.clone()),
        Result_::Image(r) => Some(r.title.clone()),
        Result_::Paper(r) => Some(r.title.clone()),
        Result_::Code(r) => Some(r.title.clone()),
        Result_::File(r) => Some(r.title.clone()),
        Result_::KeyValue(r) => Some(r.title.clone()),
        _ => None,
    }
}

fn set_result_title(result: &mut Result_, value: String) {
    match result {
        Result_::Main(r) => r.title = value,
        Result_::Image(r) => r.title = value,
        Result_::Paper(r) => r.title = value,
        Result_::Code(r) => r.title = value,
        Result_::File(r) => r.title = value,
        Result_::KeyValue(r) => r.title = value,
        _ => {}
    }
}

fn result_content(result: &Result_) -> Option<String> {
    match result {
        Result_::Main(r) => Some(r.content.clone()),
        Result_::Image(r) => Some(r.content.clone()),
        Result_::Paper(r) => Some(r.content.clone()),
        Result_::Code(r) => Some(r.content.clone()),
        Result_::File(r) => Some(r.content.clone()),
        Result_::KeyValue(r) => Some(r.content.clone()),
        Result_::Infobox(r) => Some(r.content.clone()),
        _ => None,
    }
}

fn set_result_content(result: &mut Result_, value: String) {
    match result {
        Result_::Main(r) => r.content = value,
        Result_::Image(r) => r.content = value,
        Result_::Paper(r) => r.content = value,
        Result_::Code(r) => r.content = value,
        Result_::File(r) => r.content = value,
        Result_::KeyValue(r) => r.content = value,
        Result_::Infobox(r) => r.content = value,
        _ => {}
    }
}

fn result_engine(result: &Result_) -> String {
    match result {
        Result_::Main(r) => r.engine.clone(),
        Result_::Answer(r) => r.engine.clone(),
        Result_::Image(r) => r.engine.clone(),
        Result_::Paper(r) => r.engine.clone(),
        Result_::Code(r) => r.engine.clone(),
        Result_::File(r) => r.engine.clone(),
        Result_::KeyValue(r) => r.engine.clone(),
        Result_::Suggestion(r) => r.engine.clone(),
        Result_::Correction(r) => r.engine.clone(),
        Result_::Infobox(r) => r.engine.clone(),
    }
}

fn result_score(result: &Result_) -> Option<f64> {
    match result {
        Result_::Main(r) => Some(r.score),
        Result_::Image(r) => Some(r.score),
        Result_::Paper(r) => Some(r.score),
        Result_::Code(r) => Some(r.score),
        Result_::File(r) => Some(r.score),
        Result_::KeyValue(r) => Some(r.score),
        _ => None,
    }
}

fn set_result_score(result: &mut Result_, value: f64) {
    match result {
        Result_::Main(r) => r.score = value,
        Result_::Image(r) => r.score = value,
        Result_::Paper(r) => r.score = value,
        Result_::Code(r) => r.score = value,
        Result_::File(r) => r.score = value,
        Result_::KeyValue(r) => r.score = value,
        _ => {}
    }
}

fn result_priority(result: &Result_) -> Option<String> {
    match result {
        Result_::Main(r) => Some(r.priority.clone()),
        Result_::Image(r) => Some(r.priority.clone()),
        Result_::Paper(r) => Some(r.priority.clone()),
        Result_::Code(r) => Some(r.priority.clone()),
        Result_::File(r) => Some(r.priority.clone()),
        Result_::KeyValue(r) => Some(r.priority.clone()),
        _ => None,
    }
}

fn set_result_priority(result: &mut Result_, value: String) {
    match result {
        Result_::Main(r) => r.priority = value,
        Result_::Image(r) => r.priority = value,
        Result_::Paper(r) => r.priority = value,
        Result_::Code(r) => r.priority = value,
        Result_::File(r) => r.priority = value,
        Result_::KeyValue(r) => r.priority = value,
        _ => {}
    }
}

fn result_img_src(result: &Result_) -> Option<String> {
    match result {
        Result_::Image(r) => Some(r.img_src.clone()),
        Result_::Infobox(r) => r.img_src.clone(),
        _ => None,
    }
}

fn set_result_img_src(result: &mut Result_, value: String) {
    match result {
        Result_::Image(r) => r.img_src = value,
        Result_::Infobox(r) => r.img_src = Some(value),
        _ => {}
    }
}

fn result_thumbnail_src(result: &Result_) -> Option<String> {
    match result {
        Result_::Image(r) => Some(r.thumbnail_src.clone()),
        _ => None,
    }
}

fn set_result_thumbnail_src(result: &mut Result_, value: String) {
    if let Result_::Image(r) = result {
        r.thumbnail_src = value;
    }
}

fn result_doi(result: &Result_) -> Option<String> {
    match result {
        Result_::Paper(r) => Some(r.doi.clone()),
        _ => None,
    }
}

fn container_to_table(
    lua: &Lua,
    container: &mut dyn ResultContainerMut,
) -> mlua::Result<(Table, Vec<ResultHandle>)> {
    let table = lua.create_table()?;
    let results = lua.create_table()?;
    let mut handles = Vec::new();
    for (idx, result) in container.main_results_mut().iter().enumerate() {
        let handle = ResultHandle::new(result.clone());
        results.set(idx + 1, lua.create_userdata(handle.clone())?)?;
        handles.push(handle);
    }
    table.set("results", results)?;
    let answers = lua.create_table()?;
    for (idx, answer) in container.answers_mut().iter().enumerate() {
        answers.set(idx + 1, answer_to_table(lua, answer)?)?;
    }
    table.set("answers", answers)?;
    Ok((table, handles))
}

fn expire_handles(handles: &[ResultHandle]) {
    for handle in handles {
        let _ = handle.expire();
    }
}

fn apply_container_table(
    table: &Table,
    container: &mut dyn ResultContainerMut,
) -> mlua::Result<()> {
    if let Some(results) = table.get::<Option<Table>>("results")? {
        let mut out = Vec::new();
        for value in results.sequence_values::<Value>() {
            match value? {
                Value::UserData(userdata) => {
                    out.push(userdata.borrow::<ResultHandle>()?.snapshot()?);
                }
                Value::Table(table) => out.push(Result_::Main(main_from_table(&table)?)),
                _ => {
                    return Err(mlua::Error::external(
                        "results entries must be result handles or tables",
                    ));
                }
            }
        }
        *container.main_results_mut() = out;
    }
    if let Some(answers) = table.get::<Option<Table>>("answers")? {
        let mut out = Vec::new();
        for value in answers.sequence_values::<Table>() {
            out.push(answer_from_table(&value?)?);
        }
        *container.answers_mut() = out;
    }
    if let Some(infoboxes) = table.get::<Option<Table>>("infoboxes")? {
        let mut out = Vec::new();
        for value in infoboxes.sequence_values::<Table>() {
            out.push(infobox_from_table(&value?)?);
        }
        *container.infoboxes_mut() = out;
    }
    Ok(())
}

fn main_from_table(table: &Table) -> mlua::Result<MainResult> {
    let url = table.get::<String>("url")?;
    validate_url(&url)?;
    let score = table.get::<Option<f64>>("score")?.unwrap_or_default();
    if !score.is_finite() {
        return Err(mlua::Error::external("score must be finite"));
    }
    Ok(MainResult {
        normalized_url: url.clone(),
        url,
        title: table.get::<Option<String>>("title")?.unwrap_or_default(),
        content: table.get::<Option<String>>("content")?.unwrap_or_default(),
        engine: table
            .get::<Option<String>>("engine")?
            .unwrap_or_else(|| "lua".to_string()),
        score,
        priority: table.get::<Option<String>>("priority")?.unwrap_or_default(),
        template: Template::Default,
        ..MainResult::default()
    })
}

fn answers_from_value(value: Value) -> mlua::Result<Vec<Answer>> {
    match value {
        Value::Nil => Ok(Vec::new()),
        Value::Table(table) if table.contains_key("answer")? => {
            Ok(vec![answer_from_table(&table)?])
        }
        Value::Table(table) => table
            .sequence_values::<Table>()
            .map(|value| answer_from_table(&value?))
            .collect(),
        _ => Err(mlua::Error::external(
            "answer hook must return table or nil",
        )),
    }
}

fn answer_to_table(lua: &Lua, answer: &Answer) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("answer", answer.answer.clone())?;
    table.set("url", answer.url.clone())?;
    table.set("engine", answer.engine.clone())?;
    Ok(table)
}

fn answer_from_table(table: &Table) -> mlua::Result<Answer> {
    Ok(Answer {
        answer: table.get::<Option<String>>("answer")?.unwrap_or_default(),
        url: table.get::<Option<String>>("url")?,
        engine: table
            .get::<Option<String>>("engine")?
            .unwrap_or_else(|| "lua".to_string()),
        template: Template::Answer,
        interactive: interactive_from_table(table)?,
    })
}

fn interactive_from_table(
    table: &Table,
) -> mlua::Result<Option<zoeken_results::InteractiveAnswer>> {
    let Some(interactive) = table.get::<Option<Table>>("interactive")? else {
        return Ok(None);
    };
    let kind = interactive
        .get::<Option<String>>("type")?
        .unwrap_or_default();
    match kind.as_str() {
        "unit" => Ok(Some(zoeken_results::InteractiveAnswer::Unit {
            amount: interactive.get::<f64>("amount")?,
            from: interactive.get::<String>("from")?,
            to: interactive.get::<String>("to")?,
            result: interactive.get::<f64>("result")?,
            dimension: interactive.get::<String>("dimension")?,
        })),
        "currency" => Ok(Some(zoeken_results::InteractiveAnswer::Currency {
            amount: interactive.get::<f64>("amount")?,
            from: interactive.get::<String>("from")?,
            to: interactive.get::<String>("to")?,
            result: interactive.get::<f64>("result")?,
            rate: interactive.get::<f64>("rate")?,
        })),
        "calculator" => Ok(Some(zoeken_results::InteractiveAnswer::Calculator {
            expression: interactive.get::<String>("expression")?,
            result: interactive.get::<f64>("result")?,
        })),
        "weather" => Ok(Some(zoeken_results::InteractiveAnswer::Weather {
            place: interactive.get::<String>("place")?,
            description: interactive.get::<String>("description")?,
            temp_c: interactive.get::<String>("temp_c")?,
            temp_f: interactive.get::<String>("temp_f")?,
            feels_c: interactive.get::<String>("feels_c")?,
            wind_kmph: interactive.get::<String>("wind_kmph")?,
            wind_dir: interactive.get::<String>("wind_dir")?,
            humidity: interactive.get::<String>("humidity")?,
        })),
        "self_info" => Ok(Some(zoeken_results::InteractiveAnswer::SelfInfo {
            kind: interactive.get::<String>("kind")?,
            value: interactive.get::<String>("value")?,
        })),
        "crypto" => Ok(Some(zoeken_results::InteractiveAnswer::Crypto {
            mode: interactive.get::<String>("mode")?,
            algorithm: interactive.get::<String>("algorithm")?,
            input: interactive.get::<String>("input")?,
        })),
        "translate" => Ok(Some(zoeken_results::InteractiveAnswer::Translate {
            source: interactive.get::<String>("source")?,
            target_lang: interactive.get::<String>("target_lang")?,
            translated: interactive.get::<String>("translated")?,
        })),
        "dictionary" => {
            let definitions = interactive
                .get::<Option<Vec<String>>>("definitions")?
                .unwrap_or_default();
            Ok(Some(zoeken_results::InteractiveAnswer::Dictionary {
                term: interactive.get::<String>("term")?,
                definitions,
            }))
        }
        "wikipedia" => Ok(Some(zoeken_results::InteractiveAnswer::Wikipedia {
            title: interactive.get::<String>("title")?,
            extract: interactive
                .get::<Option<String>>("extract")?
                .unwrap_or_default(),
            description: interactive
                .get::<Option<String>>("description")?
                .unwrap_or_default(),
            img_src: interactive
                .get::<Option<String>>("img_src")?
                .unwrap_or_default(),
            url: interactive
                .get::<Option<String>>("url")?
                .unwrap_or_default(),
        })),
        "" => Ok(None),
        other => Err(mlua::Error::external(format!(
            "unknown interactive answer type: {other}"
        ))),
    }
}

fn infobox_from_table(table: &Table) -> mlua::Result<Infobox> {
    Ok(Infobox {
        infobox: table.get::<Option<String>>("infobox")?.unwrap_or_default(),
        id: table.get::<Option<String>>("id")?,
        engine: table
            .get::<Option<String>>("engine")?
            .unwrap_or_else(|| "lua".to_string()),
        ..Infobox::default()
    })
}

fn string_vec_to_table(lua: &Lua, values: &[String]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (idx, value) in values.iter().enumerate() {
        table.set(idx + 1, value.as_str())?;
    }
    Ok(table)
}

/// Membership table for Ahmia hashes without materializing ~57k keys into Lua.
fn ahmia_blacklist_to_table(
    lua: &Lua,
    blacklist: &zoeken_data::AhmiaBlacklist,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let blacklist = blacklist.clone();
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |_, (_tbl, key): (Table, String)| Ok(blacklist.contains(&key)))?,
    )?;
    table.set_metatable(Some(mt))?;
    Ok(table)
}

fn validate_url(raw: &str) -> mlua::Result<()> {
    url::Url::parse(raw)
        .map(|_| ())
        .map_err(|_| mlua::Error::external(format!("invalid url: {raw}")))
}

fn validate_optional_url(raw: &str) -> mlua::Result<()> {
    if raw.is_empty() {
        Ok(())
    } else {
        validate_url(raw)
    }
}

fn eval_expr(expr: &str) -> Option<f64> {
    let mut ns = fasteval::EmptyNamespace;
    match fasteval::ez_eval(expr, &mut ns) {
        Ok(value) if value.is_finite() => Some(value),
        _ => None,
    }
}

fn hash_digest(algorithm: &str, input: &str) -> anyhow::Result<String> {
    let bytes = input.as_bytes();
    match algorithm.to_ascii_lowercase().as_str() {
        "md5" => Ok(bytes_to_hex(Md5::digest(bytes))),
        "sha1" => Ok(bytes_to_hex(Sha1::digest(bytes))),
        "sha224" => Ok(bytes_to_hex(Sha224::digest(bytes))),
        "sha256" => Ok(bytes_to_hex(Sha256::digest(bytes))),
        "sha384" => Ok(bytes_to_hex(Sha384::digest(bytes))),
        "sha512" => Ok(bytes_to_hex(Sha512::digest(bytes))),
        other => Err(anyhow!("unsupported hash algorithm: {other}")),
    }
}

fn bytes_to_hex(digest: impl AsRef<[u8]>) -> String {
    let digest = digest.as_ref();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn now_utc() -> String {
    let dt: DateTime<Utc> = std::time::SystemTime::now().into();
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn url_host(raw: &str) -> Option<String> {
    url::Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

fn rewrite_host(raw: &str, pattern: &str, replacement: &str) -> anyhow::Result<Option<String>> {
    let mut parsed = match url::Url::parse(raw) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    let Some(host) = parsed.host_str() else {
        return Ok(None);
    };
    let re = regex::Regex::new(pattern)?;
    if !re.is_match(host) {
        return Ok(None);
    }
    let new_host = re.replace_all(host, replacement).into_owned();
    parsed
        .set_host(Some(&new_host))
        .map_err(|_| anyhow!("invalid replacement host: {new_host}"))?;
    Ok(Some(parsed.to_string()))
}

fn extract_doi(raw: &str) -> Option<String> {
    static DOI_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"10\.\d{4,9}/[^\s]+").expect("valid DOI regex"));
    let parsed = url::Url::parse(raw).ok()?;
    if let Some(found) = DOI_RE.find(parsed.path()) {
        return Some(strip_doi_suffixes(found.as_str()));
    }
    for (_, value) in parsed.query_pairs() {
        if let Some(found) = DOI_RE.find(&value) {
            return Some(strip_doi_suffixes(found.as_str()));
        }
    }
    None
}

fn normalize_ip(raw: &str) -> Option<String> {
    raw.parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| ip.to_string())
}

fn download_tor_exit_nodes(
    network: Arc<zoeken_network::NetworkManager>,
) -> anyhow::Result<Vec<String>> {
    const EXIT_LIST_URL: &str = "https://check.torproject.org/exit-addresses";
    const EXIT_LIST_TIMEOUT: Duration = Duration::from_secs(5);
    let text = std::thread::spawn(move || -> anyhow::Result<String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            tokio::time::timeout(EXIT_LIST_TIMEOUT, async {
                let response = network
                    .request(
                        zoeken_network::DEFAULT_NETWORK,
                        zoeken_network::NetworkRequest::get(EXIT_LIST_URL)
                            .with_timeout(EXIT_LIST_TIMEOUT),
                    )
                    .await?;
                Ok::<_, anyhow::Error>(response.text().await?)
            })
            .await
            .map_err(|_| anyhow!("timed out downloading Tor exit node list"))?
        })
    })
    .join()
    .map_err(|_| anyhow!("Tor exit node worker panicked"))??;
    let re = regex::Regex::new(r"(?m)^ExitAddress\s+(\S+)")?;
    Ok(re
        .captures_iter(&text)
        .filter_map(|capture| capture.get(1).map(|m| m.as_str().to_string()))
        .collect())
}

fn strip_doi_suffixes(raw: &str) -> String {
    let mut doi = raw.to_string();
    for suffix in ["/", ".pdf", ".xml", "/full", "/meta", "/abstract"] {
        if let Some(trimmed) = doi.strip_suffix(suffix) {
            doi = trimmed.to_string();
        }
    }
    doi
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_data::HostnamesRules;

    use crate::SimpleResultContainer;

    fn plugin(source: &str) -> LuaPlugin {
        LuaPlugin::from_source(
            "test",
            source,
            Arc::new(zoeken_data::DataBundle::default()),
            LuaRuntimeConfig::default(),
        )
        .expect("load plugin")
    }

    fn plugin_error(source: &str) -> LuaPluginError {
        match LuaPlugin::from_source(
            "test",
            source,
            Arc::new(zoeken_data::DataBundle::default()),
            LuaRuntimeConfig::default(),
        ) {
            Ok(_) => panic!("plugin should fail to load"),
            Err(error) => error,
        }
    }

    fn builtin(id: &str, data: zoeken_data::DataBundle) -> LuaPlugin {
        LuaPlugin::from_file(
            &builtins_dir().join(format!("{id}.lua")),
            Arc::new(data),
            LuaRuntimeConfig::default(),
        )
        .expect("load builtin plugin")
    }

    fn builtins_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")
    }

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            query: text.to_string(),
            ..SearchQuery::default()
        }
    }

    fn main(url: &str, title: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            ..MainResult::default()
        })
    }

    #[test]
    fn metadata_parses_and_answer_hook_runs() {
        let plugin = plugin(
            r#"
            return {
              id = "answer_lua",
              name = "Answer Lua",
              api_version = 1,
              kind = "answerer",
              keywords = {"lua"},
              capabilities = {"answers"},
              answer = function(query, ctx)
                return {{ answer = "hello " .. query.query, engine = "answer_lua" }}
              end
            }
            "#,
        );
        assert_eq!(plugin.info().id, "answer_lua");
        let query = SearchQuery {
            query: "lua".to_string(),
            ..SearchQuery::default()
        };
        let answers = plugin.on_pre_search_answers(&query, &PluginCtx::all_enabled());
        assert_eq!(answers[0].answer, "hello lua");
    }

    #[test]
    fn result_hook_mutates_and_can_drop() {
        let plugin = plugin(
            r#"
            return {
              id = "result_lua",
              api_version = 1,
              kind = "result_plugin",
              capabilities = {"result"},
              on_result = function(result, query, ctx)
                result.title = result.title .. "!"
                return result.url ~= "https://drop.test/"
              end
            }
            "#,
        );
        let query = SearchQuery::default();
        let mut keep = Result_::Main(MainResult {
            url: "https://keep.test/".to_string(),
            title: "Keep".to_string(),
            ..MainResult::default()
        });
        assert!(plugin.on_result(&mut keep, &query, &PluginCtx::all_enabled()));
        assert!(matches!(keep, Result_::Main(ref main) if main.title == "Keep!"));

        let mut drop = Result_::Main(MainResult {
            url: "https://drop.test/".to_string(),
            title: "Drop".to_string(),
            ..MainResult::default()
        });
        assert!(!plugin.on_result(&mut drop, &query, &PluginCtx::all_enabled()));
    }

    #[test]
    fn post_search_appends_result() {
        let plugin = plugin(
            r#"
            return {
              id = "append_lua",
              api_version = 1,
              kind = "result_plugin",
              capabilities = {"results"},
              post_search = function(results, ctx)
                table.insert(results.results, {url = "https://lua.test/", title = "Lua"})
              end
            }
            "#,
        );
        let mut container = SimpleResultContainer::default();
        plugin.on_post_search(&mut container, &PluginCtx::all_enabled());
        assert_eq!(container.results.len(), 1);
    }

    #[test]
    fn invalid_url_write_is_isolated() {
        let plugin = plugin(
            r#"
            return {
              id = "bad_lua",
              api_version = 1,
              kind = "result_plugin",
              capabilities = {"result"},
              on_result = function(result, query, ctx)
                result.url = "not a url"
              end
            }
            "#,
        );
        let query = SearchQuery::default();
        let mut result = Result_::Main(MainResult {
            url: "https://ok.test/".to_string(),
            ..MainResult::default()
        });
        assert!(plugin.on_result(&mut result, &query, &PluginCtx::all_enabled()));
        assert!(matches!(result, Result_::Main(ref main) if main.url == "https://ok.test/"));
        assert_eq!(plugin.metrics().hook_failures.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn normalized_url_is_read_only() {
        let plugin = plugin(
            r#"
            return {
              id = "readonly_lua",
              api_version = 1,
              kind = "result_plugin",
              capabilities = {"result"},
              on_result = function(result, query, ctx)
                result.normalized_url = "https://changed.test/"
              end
            }
            "#,
        );
        let query = SearchQuery::default();
        let mut result = main("https://ok.test/", "Ok");
        assert!(plugin.on_result(&mut result, &query, &PluginCtx::all_enabled()));
        assert!(
            matches!(result, Result_::Main(ref main) if main.normalized_url == "https://ok.test/")
        );
        assert_eq!(plugin.metrics().hook_failures.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn retained_result_handle_is_stale_after_hook() {
        let plugin = plugin(
            r#"
            local retained
            return {
              id = "stale_lua",
              api_version = 1,
              kind = "result_plugin",
              capabilities = {"result"},
              on_result = function(result, query, ctx)
                if retained then
                  local ok = pcall(function() retained.title = "late" end)
                  if ok then error("retained handle stayed live") end
                end
                retained = result
                result.title = result.title .. "!"
                return true
              end
            }
            "#,
        );
        let query = SearchQuery::default();
        let mut first = main("https://first.test/", "First");
        let mut second = main("https://second.test/", "Second");
        assert!(plugin.on_result(&mut first, &query, &PluginCtx::all_enabled()));
        assert!(plugin.on_result(&mut second, &query, &PluginCtx::all_enabled()));
        assert!(matches!(first, Result_::Main(ref main) if main.title == "First!"));
        assert!(matches!(second, Result_::Main(ref main) if main.title == "Second!"));
    }

    #[test]
    fn metadata_validation_rejects_non_scalar_fields() {
        let error = plugin_error(
            r#"
            return {
              id = "bad_meta",
              name = {"not", "scalar"},
              api_version = 1,
              kind = "answerer",
              capabilities = {"answers"}
            }
            "#,
        );
        assert!(matches!(error, LuaPluginError::InvalidMetadata { .. }));

        let error = plugin_error(
            r#"
            return {
              id = "bad_keywords",
              api_version = 1,
              kind = "answerer",
              keywords = {"ok", 3},
              capabilities = {"answers"}
            }
            "#,
        );
        assert!(matches!(error, LuaPluginError::InvalidMetadata { .. }));
    }

    #[test]
    fn sandbox_removes_filesystem_and_process_globals() {
        let plugin = plugin(
            r#"
            return {
              id = "sandbox_lua",
              api_version = 1,
              kind = "answerer",
              capabilities = {"answers"},
              answer = function(query, ctx)
                if io ~= nil or os ~= nil or require ~= nil or debug ~= nil then
                  error("dangerous global exposed")
                end
                return {{ answer = "sandboxed" }}
              end
            }
            "#,
        );
        let answers =
            plugin.on_pre_search_answers(&SearchQuery::default(), &PluginCtx::all_enabled());
        assert_eq!(answers[0].answer, "sandboxed");
    }

    #[test]
    fn init_can_self_disable_plugin() {
        let plugin = plugin(
            r#"
            return {
              id = "disabled_lua",
              api_version = 1,
              kind = "answerer",
              capabilities = {"answers"},
              init = function(ctx) return false end,
              answer = function(query, ctx)
                return {{ answer = "should not run" }}
              end
            }
            "#,
        );
        let answers =
            plugin.on_pre_search_answers(&SearchQuery::default(), &PluginCtx::all_enabled());
        assert!(answers.is_empty());
    }

    #[test]
    fn external_directory_loader_skips_broken_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("good.lua"),
            r#"
            return {
              id = "good_lua",
              api_version = 1,
              kind = "answerer",
              capabilities = {"answers"},
              answer = function(query, ctx) return {{ answer = "ok" }} end
            }
            "#,
        )
        .expect("write good");
        std::fs::write(dir.path().join("bad.lua"), "return { api_version = 999 }")
            .expect("write bad");

        let plugins = load_plugins_from_dir(
            dir.path(),
            Arc::new(zoeken_data::DataBundle::default()),
            LuaRuntimeConfig::default(),
        );
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id(), "good_lua");
    }

    #[test]
    fn standard_lua_builtin_set_loads_all_plugins() {
        let plugins = load_plugins_from_dir(
            &builtins_dir(),
            Arc::new(zoeken_data::DataBundle::default()),
            LuaRuntimeConfig::default(),
        );
        let ids: Vec<_> = plugins.iter().map(|plugin| plugin.id()).collect();
        assert_eq!(
            ids,
            vec![
                "calculator",
                "unit_converter",
                "self_info",
                "time_zone",
                "tracker_url_remover",
                "hostnames",
                "oa_doi_rewrite",
                "ahmia_filter",
                "tor_check",
                "infiniteScroll",
            ]
        );
    }

    #[test]
    fn builtin_answerers_run_from_lua() {
        let data = zoeken_data::DataBundle::default();
        let calc = builtin("calculator", data.clone());
        let answers = calc.on_pre_search_answers(&query("2 + 2"), &PluginCtx::all_enabled());
        assert_eq!(answers[0].answer, "4");

        let ctx = PluginCtx::all_enabled()
            .with_client_ip("192.0.2.5")
            .with_user_agent("Mozilla/5.0");
        let self_info = builtin("self_info", data);
        let answers = self_info.on_pre_search_answers(&query("ip"), &ctx);
        assert_eq!(answers[0].answer, "Your IP is: 192.0.2.5");
        match &answers[0].interactive {
            Some(zoeken_results::InteractiveAnswer::SelfInfo { kind, value }) => {
                assert_eq!(kind, "ip");
                assert_eq!(value, "192.0.2.5");
            }
            other => panic!("expected SelfInfo interactive, got {other:?}"),
        }

        let answers = self_info.on_pre_search_answers(&query("whats my ip"), &ctx);
        assert_eq!(answers[0].answer, "Your IP is: 192.0.2.5");
        assert!(matches!(
            &answers[0].interactive,
            Some(zoeken_results::InteractiveAnswer::SelfInfo { kind, .. }) if kind == "ip"
        ));
    }

    #[test]
    fn builtin_unit_converter_converts_from_its_own_curated_units() {
        // Self-contained (no ctx.data.units — that Wikidata dump has no "cup"
        // and maps "gal" to the acceleration unit, not gallon).
        let data = zoeken_data::DataBundle::default();
        let plugin = builtin("unit_converter", data);
        let answers = plugin.on_pre_search_answers(&query("2 km in m"), &PluginCtx::all_enabled());
        assert_eq!(answers[0].answer, "2 km = 2000 m");
    }

    #[test]
    fn builtin_unit_converter_understands_how_many_phrasing() {
        let data = zoeken_data::DataBundle::default();
        let plugin = builtin("unit_converter", data);
        let answers = plugin.on_pre_search_answers(
            &query("how many cups in a gallon"),
            &PluginCtx::all_enabled(),
        );
        assert_eq!(answers[0].answer, "1 gal = 16 cup");
    }

    #[test]
    fn builtin_unit_converter_treats_oz_as_floz_with_volume() {
        let data = zoeken_data::DataBundle::default();
        let plugin = builtin("unit_converter", data);
        let answers =
            plugin.on_pre_search_answers(&query("how many oz in a gal"), &PluginCtx::all_enabled());
        assert_eq!(answers[0].answer, "1 gal = 128 floz");
        match &answers[0].interactive {
            Some(zoeken_results::InteractiveAnswer::Unit {
                from,
                to,
                result,
                dimension,
                ..
            }) => {
                assert_eq!(from, "gal");
                assert_eq!(to, "floz");
                assert!((*result - 128.0).abs() < 1e-9);
                assert_eq!(dimension, "volume");
            }
            other => panic!("expected unit interactive payload, got {other:?}"),
        }
    }

    #[test]
    fn builtin_result_plugins_mutate_and_filter_results() {
        let data = zoeken_data::load_embedded_bundle().expect("embedded data");
        let tracker = builtin("tracker_url_remover", data);
        let mut result = main("https://example.com/a?utm_source=x&q=rust", "Title");
        tracker.on_result(
            &mut result,
            &SearchQuery::default(),
            &PluginCtx::all_enabled(),
        );
        assert!(matches!(result, Result_::Main(ref r) if r.url == "https://example.com/a?q=rust"));

        let tor = builtin("tor_check", zoeken_data::DataBundle::default());
        let answers = tor.on_pre_search_answers(&query("not tor"), &PluginCtx::all_enabled());
        assert!(answers.is_empty());
    }

    #[test]
    fn builtin_doi_and_hostnames_use_runtime_plugin_data() {
        let data = zoeken_data::DataBundle {
            plugin_data: zoeken_data::PluginData {
                doi_resolver: Some("https://oa.example/".to_string()),
                hostnames: HostnamesRules {
                    replace: vec![(
                        "(.*\\.)?youtube\\.com$".to_string(),
                        "invidious.example.com".to_string(),
                    )],
                    remove: vec!["(.*\\.)?facebook\\.com$".to_string()],
                    high_priority: vec!["(.*\\.)?wikipedia\\.org$".to_string()],
                    low_priority: vec!["(.*\\.)?google\\.com$".to_string()],
                },
                using_tor_proxy: false,
            },
            ..zoeken_data::DataBundle::default()
        };
        let doi = builtin("oa_doi_rewrite", data.clone());
        let mut result = main("https://publisher.example/10.1000/xyz123.pdf", "Paper");
        doi.on_result(
            &mut result,
            &SearchQuery::default(),
            &PluginCtx::all_enabled(),
        );
        assert!(
            matches!(result, Result_::Main(ref r) if r.url == "https://oa.example/10.1000/xyz123")
        );

        let hostnames = builtin("hostnames", data);
        let mut result = main("https://www.youtube.com/watch?v=abc", "Video");
        assert!(hostnames.on_result(
            &mut result,
            &SearchQuery::default(),
            &PluginCtx::all_enabled()
        ));
        assert!(
            matches!(result, Result_::Main(ref r) if r.url == "https://invidious.example.com/watch?v=abc")
        );

        let mut result = main("https://m.facebook.com/page", "Drop");
        assert!(!hostnames.on_result(
            &mut result,
            &SearchQuery::default(),
            &PluginCtx::all_enabled()
        ));

        let mut result = main("https://www.google.com/search?q=x", "Low");
        assert!(hostnames.on_result(
            &mut result,
            &SearchQuery::default(),
            &PluginCtx::all_enabled()
        ));
        assert!(matches!(result, Result_::Main(ref r) if r.priority == "low"));

        let mut result = main("https://en.wikipedia.org/wiki/Rust", "High");
        assert!(hostnames.on_result(
            &mut result,
            &SearchQuery::default(),
            &PluginCtx::all_enabled()
        ));
        assert!(matches!(result, Result_::Main(ref r) if r.priority == "high"));
    }

    #[test]
    fn builtin_ahmia_filter_removes_blocklisted_onion() {
        let mut data = zoeken_data::DataBundle::default();
        data.plugin_data.using_tor_proxy = true;
        data.ahmia_blacklist
            .insert(hash_digest("md5", "blockedxxxxxxxxx.onion").unwrap());
        let plugin = builtin("ahmia_filter", data);
        let query = SearchQuery::default();
        let mut blocked = main("http://blockedxxxxxxxxx.onion/page", "Blocked");
        let mut safe = main("http://safeonionxxxxxx.onion/page", "Safe");
        let mut regular = main("https://example.com/", "Example");
        assert!(!plugin.on_result(&mut blocked, &query, &PluginCtx::all_enabled()));
        assert!(plugin.on_result(&mut safe, &query, &PluginCtx::all_enabled()));
        assert!(plugin.on_result(&mut regular, &query, &PluginCtx::all_enabled()));
    }

    #[test]
    fn instruction_budget_isolates_runaway_hook() {
        let plugin = LuaPlugin::from_source(
            "loop",
            r#"
            return {
              id = "loop",
              api_version = 1,
              kind = "result_plugin",
              capabilities = {"result"},
              on_result = function(result, query, ctx)
                while true do end
              end
            }
            "#,
            Arc::new(zoeken_data::DataBundle::default()),
            LuaRuntimeConfig {
                instruction_budget: 5_000,
                hook_timeout: Duration::from_secs(2),
                vm_pool_size: 1,
                ..LuaRuntimeConfig::default()
            },
        )
        .expect("plugin loads");
        let query = SearchQuery::default();
        let mut result = Result_::Main(MainResult {
            url: "https://ok.test/".to_string(),
            ..MainResult::default()
        });
        assert!(plugin.on_result(&mut result, &query, &PluginCtx::all_enabled()));
        assert!(matches!(result, Result_::Main(ref main) if main.url == "https://ok.test/"));
        assert!(plugin.metrics().hook_failures.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn embedded_data_hooks_do_not_oom_or_timeout_per_result() {
        let data = Arc::new(zoeken_data::load_embedded_bundle().expect("embedded data"));
        let unit = builtin("unit_converter", (*data).clone());
        let answers = unit.on_pre_search_answers(&query("rust lang"), &PluginCtx::all_enabled());
        assert!(answers.is_empty());
        assert_eq!(unit.metrics().hook_failures.load(Ordering::Relaxed), 0);
        assert_eq!(unit.metrics().timeouts.load(Ordering::Relaxed), 0);

        let doi = builtin("oa_doi_rewrite", (*data).clone());
        let tracker = builtin("tracker_url_remover", (*data).clone());
        let search_query = SearchQuery::default();
        for i in 0..40 {
            let mut result = main(
                &format!("https://example.com/path/{i}?utm_source=x&q=rust"),
                "Title",
            );
            assert!(doi.on_result(&mut result, &search_query, &PluginCtx::all_enabled()));
            assert!(tracker.on_result(&mut result, &search_query, &PluginCtx::all_enabled()));
            assert!(
                matches!(result, Result_::Main(ref r) if !r.url.contains("utm_source")),
                "tracker should strip utm params"
            );
        }
        assert_eq!(doi.metrics().hook_failures.load(Ordering::Relaxed), 0);
        assert_eq!(doi.metrics().timeouts.load(Ordering::Relaxed), 0);
        assert_eq!(tracker.metrics().hook_failures.load(Ordering::Relaxed), 0);
        assert_eq!(tracker.metrics().timeouts.load(Ordering::Relaxed), 0);
    }
}
