-- Self-info answerer: IP / user-agent for short self-referential queries.

local function normalize(q)
  q = (q or ""):lower()
  q = q:gsub("%?", "")
  q = q:gsub("%s+", " ")
  q = q:match("^%s*(.-)%s*$") or ""
  return q
end

-- Whole-query phrases only — avoids matching "ip" inside arbitrary sentences.
local IP_QUERIES = {
  ["ip"] = true,
  ["my ip"] = true,
  ["ip address"] = true,
  ["my ip address"] = true,
  ["whats my ip"] = true,
  ["what's my ip"] = true,
  ["what is my ip"] = true,
  ["whats my ip address"] = true,
  ["what's my ip address"] = true,
  ["what is my ip address"] = true,
  ["show my ip"] = true,
  ["show my ip address"] = true,
}

local UA_QUERIES = {
  ["user-agent"] = true,
  ["user agent"] = true,
  ["my user-agent"] = true,
  ["my user agent"] = true,
  ["whats my user-agent"] = true,
  ["whats my user agent"] = true,
  ["what's my user-agent"] = true,
  ["what's my user agent"] = true,
  ["what is my user-agent"] = true,
  ["what is my user agent"] = true,
  ["show my user-agent"] = true,
  ["show my user agent"] = true,
}

return {
  id = "self_info",
  name = "Self Information",
  description = "Displays your IP or user agent for queries like \"whats my ip\" / \"user-agent\".",
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  keywords = {"ip", "user-agent", "user"},
  preference_section = "query",
  order = 40,
  capabilities = {"answers", "query", "request"},
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local q = normalize(query.query)

    if IP_QUERIES[q] then
      local ip = ctx.client_ip
      if ip and ip ~= "" then
        return {
          answer = "Your IP is: " .. ip,
          engine = "self_info",
          interactive = { type = "self_info", kind = "ip", value = ip },
        }
      end
      return {
        answer = "Your IP is unavailable",
        engine = "self_info",
        interactive = { type = "self_info", kind = "ip", value = "" },
      }
    end

    if UA_QUERIES[q] then
      local ua = ctx.user_agent
      if ua and ua ~= "" then
        return {
          answer = "Your user-agent is: " .. ua,
          engine = "self_info",
          interactive = { type = "self_info", kind = "user_agent", value = ua },
        }
      end
      return {
        answer = "Your user-agent is unavailable",
        engine = "self_info",
        interactive = { type = "self_info", kind = "user_agent", value = "" },
      }
    end

    return nil
  end,
}
