local blocklist = {}

return {
  id = "ahmia_filter",
  name = "Ahmia blacklist",
  description = "Filter out onion results that appear in Ahmia's blacklist.",
  api_version = 1,
  kind = "result_plugin",
  default_enabled = true,
  preference_section = "general",
  order = 90,
  capabilities = {"data", "result", "utils"},
  init = function(ctx)
    if not ctx.data.using_tor_proxy then return false end
    blocklist = ctx.data.ahmia_blacklist or {}
    return true
  end,
  on_result = function(result, query, ctx)
    local host = result.url and ctx.utils.url_host(result.url)
    if not host or not host:match("%.onion$") then return true end
    return not blocklist[ctx.utils.md5(host)]
  end,
}
