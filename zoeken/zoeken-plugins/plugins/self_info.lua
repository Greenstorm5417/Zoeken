return {
  id = "self_info",
  name = "Self Information",
  description = "Displays your IP if the query is \"ip\" and your user agent if the query is \"user-agent\".",
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  keywords = {"ip", "user-agent"},
  preference_section = "query",
  order = 40,
  capabilities = {"answers", "query", "request"},
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local q = (query.query or ""):lower()
    if q:match("^%s*user%-agent") and ctx.user_agent then
      return { answer = "Your user-agent is: " .. ctx.user_agent, engine = "self_info" }
    end
    if q:match("^%s*ip") and ctx.client_ip then
      return { answer = "Your IP is: " .. ctx.client_ip, engine = "self_info" }
    end
    return nil
  end,
}
