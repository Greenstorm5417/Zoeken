local units_by_symbol = {}

local function add_unit(entry)
  if not entry.symbol or entry.symbol == "" or not entry.si_name or not entry.to_si_factor then
    return
  end
  units_by_symbol[entry.symbol] = units_by_symbol[entry.symbol] or {}
  table.insert(units_by_symbol[entry.symbol], entry)
end

local function parse_measure(text)
  local number, unit = text:match("^%s*([%+%-]?[%d%,%.]+[eE]?[%+%-]?%d*)%s*(%S+)%s*$")
  if not number or not unit then return nil end
  local value = tonumber((number:gsub(",", "")))
  if not value then return nil end
  return value, unit
end

local function convert(value, from_symbol, to_symbol)
  local sources, targets = units_by_symbol[from_symbol], units_by_symbol[to_symbol]
  if not sources or not targets then return nil end
  for _, source in ipairs(sources) do
    for _, target in ipairs(targets) do
      if source.si_name == target.si_name and target.to_si_factor ~= 0 then
        return value * source.to_si_factor / target.to_si_factor
      end
    end
  end
  return nil
end

local function format_number(value)
  if value == math.floor(value) and math.abs(value) < 1000000000000000 then
    return string.format("%.0f", value)
  end
  return tostring(value)
end

return {
  id = "unit_converter",
  name = "Unit converter plugin",
  description = "Convert between units",
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  preference_section = "general",
  order = 20,
  capabilities = {"answers", "data", "query"},
  init = function(ctx)
    for _, entry in ipairs(ctx.data.units or {}) do add_unit(entry) end
    return true
  end,
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local left, to_symbol = (query.query or ""):match("^(.-)%s+[iI][nN]%s+(%S+)%s*$")
    if not left then left, to_symbol = (query.query or ""):match("^(.-)%s+[tT][oO]%s+(%S+)%s*$") end
    if not left then left, to_symbol = (query.query or ""):match("^(.-)%s+[aA][sS]%s+(%S+)%s*$") end
    if not left then return nil end
    local value, from_symbol = parse_measure(left)
    if not value then return nil end
    local result = convert(value, from_symbol, to_symbol)
    if not result then return nil end
    return { answer = format_number(result) .. " " .. to_symbol, engine = "unit_converter" }
  end,
}
