local keywords = { time=true, timezone=true, now=true, clock=true, timezones=true }

return {
  id = "time_zone",
  name = "Timezones plugin",
  description = "Display the current time on different time zones.",
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  keywords = {"time", "timezone", "now", "clock", "timezones"},
  preference_section = "query",
  examples = {"time Berlin", "clock Los Angeles"},
  order = 50,
  capabilities = {"answers", "query", "utils"},
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local found, residual = false, {}
    for part in (query.query or ""):gmatch("%S+") do
      if keywords[part:lower()] then found = true else table.insert(residual, part) end
    end
    if not found or #residual > 0 then return nil end
    return { answer = ctx.utils.now_utc(), engine = "time_zone" }
  end,
}
