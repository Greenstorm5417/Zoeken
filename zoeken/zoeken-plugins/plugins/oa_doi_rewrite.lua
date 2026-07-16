local resolver = "https://oadoi.org/"

return {
  id = "oa_doi_rewrite",
  name = "Open Access DOI rewrite",
  description = "Avoid paywalls by redirecting to open-access versions of publications when available",
  api_version = 1,
  kind = "result_plugin",
  default_enabled = false,
  preference_section = "general",
  order = 80,
  capabilities = {"data", "result", "utils"},
  init = function(ctx)
    resolver = ctx.data.doi_resolver or resolver
    return true
  end,
  on_result = function(result, query, ctx)
    if not result.url then return true end
    local doi = ctx.utils.extract_doi(result.url)
    if not doi or #doi >= 50 then return true end
    local rewritten = resolver .. doi
    result.url = rewritten
    if result.kind == "paper" and (not result.doi or result.doi == "") then result.doi = doi end
    return true
  end,
}
