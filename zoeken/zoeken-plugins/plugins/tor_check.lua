return {
  id = "tor_check",
  name = "Tor check plugin",
  description = "This plugin checks if the address of the request is a Tor exit-node, and informs the user if it is; like check.torproject.org, but from SearXNG.",
  api_version = 1,
  kind = "answerer",
  default_enabled = false,
  keywords = {"tor-check", "tor_check", "torcheck", "tor", "tor check"},
  preference_section = "query",
  order = 100,
  capabilities = {"answers", "query", "request", "utils"},
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local q = (query.query or ""):lower()
    local matched = false
    for _, keyword in ipairs({"tor-check", "tor_check", "torcheck", "tor", "tor check"}) do
      if q == keyword then matched = true end
    end
    if not matched or not ctx.client_ip then return nil end

    local ok, nodes = pcall(ctx.utils.tor_exit_nodes)
    if not ok then
      return { answer = "Could not download the list of Tor exit-nodes from https://check.torproject.org/exit-addresses", engine = "tor_check" }
    end

    local real_ip = ctx.utils.normalize_ip(ctx.client_ip) or ctx.client_ip
    for _, node in ipairs(nodes or {}) do
      if node == real_ip then
        return { answer = "You are using Tor and it looks like you have the external IP address " .. real_ip, engine = "tor_check" }
      end
    end
    return { answer = "You are not using Tor and you have the external IP address " .. real_ip, engine = "tor_check" }
  end,
}
