local algorithms = { md5=true, sha1=true, sha224=true, sha256=true, sha384=true, sha512=true }

return {
  id = "hash_plugin",
  name = "Hash plugin",
  description = "Converts strings to different hash digests. Available functions: md5, sha1, sha224, sha256, sha384, sha512.",
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  keywords = {"md5", "sha1", "sha224", "sha256", "sha384", "sha512"},
  preference_section = "query",
  examples = {"sha512 The quick brown fox jumps over the lazy dog"},
  order = 30,
  capabilities = {"answers", "query", "utils"},
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local alg, text = (query.query or ""):match("^%s*(%S+)%s+(.+)%s*$")
    if not alg then return nil end
    alg = alg:lower()
    if not algorithms[alg] then return nil end
    return { answer = alg .. " hash digest: " .. ctx.utils.hash(alg, text), engine = "hash_plugin" }
  end,
}
