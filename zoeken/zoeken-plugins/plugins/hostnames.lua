local rules = { replace = {}, remove = {}, high_priority = {}, low_priority = {} }

local function any_match(host, patterns, utils)
  for _, pattern in ipairs(patterns or {}) do if utils.regex_match(host, pattern) then return true end end
  return false
end

local function has_rules()
  return #(rules.replace or {}) > 0
      or #(rules.remove or {}) > 0
      or #(rules.high_priority or {}) > 0
      or #(rules.low_priority or {}) > 0
end

local function filter_url(result, field, ctx)
  local value = result[field]
  if not value or value == "" then return true end
  local host = ctx.utils.url_host(value)
  if host and any_match(host, rules.remove, ctx.utils) then
    result[field] = ""
    return true
  end
  for _, rule in ipairs(rules.replace or {}) do
    local rewritten = ctx.utils.rewrite_host(value, rule.pattern, rule.replacement)
    if rewritten then result[field] = rewritten end
  end
  return true
end

return {
  id = "hostnames",
  name = "Hostnames plugin",
  description = "Rewrite hostnames and remove or prioritize results based on the hostname",
  api_version = 1,
  kind = "result_plugin",
  default_enabled = true,
  preference_section = "general",
  order = 70,
  capabilities = {"data", "result", "utils"},
  init = function(ctx)
    rules = ctx.data.hostnames or rules
    return has_rules()
  end,
  on_result = function(result, query, ctx)
    local host = result.url and ctx.utils.url_host(result.url)
    if not host or any_match(host, rules.remove, ctx.utils) then return host == nil end
    filter_url(result, "url", ctx)
    filter_url(result, "img_src", ctx)
    filter_url(result, "thumbnail_src", ctx)
    filter_url(result, "thumbnail", ctx)
    if any_match(host, rules.low_priority, ctx.utils) then result.priority = "low" end
    if any_match(host, rules.high_priority, ctx.utils) then result.priority = "high" end
    return true
  end,
}
