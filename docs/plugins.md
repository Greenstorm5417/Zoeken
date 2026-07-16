# Lua Plugins

Lua plugins are admin-installed `.lua` modules loaded at startup when
`lua_plugins.enabled` is true. The default directory is `zoeken/zoeken-plugins/plugins/`,
or set `lua_plugins.directory` in `settings.yml`.

Each module returns a table:

```lua
return {
  id = "example",
  name = "Example",
  api_version = 1,
  kind = "answerer", -- answerer, result_plugin, or both
  default_enabled = true,
  keywords = {"example"},
  order = 10,
  after = {"other_plugin"},
  before = {"final_plugin"},
  capabilities = {"answers", "query"},
  answer = function(query, ctx)
    return {{ answer = "hello", engine = "example" }}
  end
}
```

Supported hooks are `init(ctx)`, `pre_search(query, ctx)`,
`pre_search_answers(query, ctx)`, `answer(query, ctx)`,
`on_result(result, query, ctx)`, `on_results(results, query, ctx)`, and
`post_search(results, ctx)`. Returning `false` from `pre_search` cancels engine
execution. Returning `false` from `on_result` drops that result.

`query` is passed as a bounded table. Existing results are passed as Rust-backed
`UserData` handles with validated setters; `normalized_url` is read-only, and a
retained result handle becomes stale after its hook returns. Container hooks expose
existing entries as result handles and still accept plain tables for appended main
results.

The runtime removes direct filesystem/process APIs (`io`, `os`, `package`,
`require`, `dofile`, `loadfile`, `load`, and `debug`) and catches load/init/hook
errors so one broken plugin cannot abort search. This is defense-in-depth for
admin-installed plugins, not a hard sandbox for hostile code; fully untrusted
plugins require process/container isolation.

Plugin state is VM-local. `init` runs once per VM, shared Rust data is exposed
read-only through `ctx.data`, and request data is passed per hook. The standard
plugin set in `zoeken/zoeken-plugins/plugins/` is implemented in Lua; `zoeken-plugins` only owns the
shared trait and registry layer. The standard set includes the server-side
answer/result plugins plus the `infiniteScroll` UI preference plugin.

Execution order is deterministic. `lua_plugins.order` takes priority, then
numeric `order`, then plugin `id`; `before` and `after` add dependency edges.
Dependency cycles are reported and the loader falls back to the stable base
order.
