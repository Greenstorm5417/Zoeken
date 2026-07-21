local comma_decimal_langs = {
  de=true, fr=true, es=true, it=true, nl=true, pt=true, ru=true, pl=true,
  sv=true, da=true, fi=true, nb=true, nn=true, no=true, cs=true, sk=true,
  sl=true, hr=true, sr=true, uk=true, bg=true, ro=true, hu=true, tr=true,
  el=true, lt=true, lv=true, et=true, is=true, ca=true, gl=true, eu=true,
  af=true, id=true,
}

local function looks_like_expression(text)
  if text == "" then return false end
  local has_digit, has_operator = false, false
  for i = 1, #text do
    local ch = text:sub(i, i)
    if ch:match("%d") then
      has_digit = true
    elseif ch:match("[+%-%*/%%%^%(%)%., ]") then
      if ch:match("[+%-%*/%%%^]") then has_operator = true end
    else
      return false
    end
  end
  return has_digit and has_operator
end

local function normalize(expr, locale)
  local lang = (locale or ""):match("^([^%-_]+)") or ""
  if comma_decimal_langs[lang:lower()] then
    return expr:gsub("%.", ""):gsub(",", ".")
  end
  return expr:gsub(",", "")
end

local function format_number(value)
  if value == math.floor(value) and math.abs(value) < 1000000000000000 then
    return string.format("%.0f", value)
  end
  return tostring(value)
end

return {
  id = "calculator",
  name = "Calculator",
  description = "Parses and solves mathematical expressions.",
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  preference_section = "query",
  order = 10,
  capabilities = {"answers", "query", "utils"},
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local expr = (query.query or ""):match("^%s*(.-)%s*$")
    if not looks_like_expression(expr) then return nil end
    local normalized = normalize(expr, query.locale)
    local value = ctx.utils.eval(normalized)
    if value == nil then return nil end
    return {
      answer = format_number(value),
      engine = "calculator",
      interactive = {
        type = "calculator",
        expression = normalized,
        result = value,
      },
    }
  end,
}
