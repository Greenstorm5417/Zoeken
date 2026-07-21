-- alias (lower) -> list of unit entries (oz is mass+volume)
local units_by_alias = {}
local phrase_aliases = {} -- multi-word phrases, longest first

local function index_alias(alias, unit)
  local key = alias:lower()
  if key:find("%s") then
    phrase_aliases[#phrase_aliases + 1] = { phrase = key, unit = unit }
  end
  local bucket = units_by_alias[key]
  if not bucket then
    units_by_alias[key] = { unit }
  else
    for _, existing in ipairs(bucket) do
      if existing.id == unit.id then return end
    end
    bucket[#bucket + 1] = unit
  end
end

for _, unit in ipairs(UNITS) do
  index_alias(unit.id, unit)
  for _, alias in ipairs(unit.abbreviations) do
    index_alias(alias, unit)
  end
end

table.sort(phrase_aliases, function(a, b)
  return #a.phrase > #b.phrase
end)

local trailing_filler = {
  please = true, pls = true, thanks = true, thank = true, now = true,
}

local function strip_trailing_noise(text)
  text = text:gsub("[%?%!%.,]+%s*$", "")
  while true do
    local head, last = text:match("^(.-)%s+(%S+)$")
    if not last then break end
    local bare = last:gsub("[%?%!%.,]+$", "")
    if trailing_filler[bare:lower()] then
      text = head
    elseif bare ~= last then
      text = head .. " " .. bare
      break
    else
      break
    end
  end
  return text
end

-- Collapse multi-word unit phrases ("fl oz", "fluid ounce") to unit ids.
local function normalize_phrases(text)
  local lower = text:lower()
  for _, entry in ipairs(phrase_aliases) do
    local start = 1
    while true do
      local i, j = lower:find(entry.phrase, start, true)
      if not i then break end
      local before = text:sub(1, i - 1)
      local after = text:sub(j + 1)
      text = before .. entry.unit.id .. after
      lower = text:lower()
      start = i + #entry.unit.id
    end
  end
  return text
end

local function lookup_all(raw)
  return units_by_alias[(raw or ""):lower()]
end

-- Prefer a candidate matching preferred_dimension when alias is ambiguous (oz).
local function lookup(raw, preferred_dimension)
  local candidates = lookup_all(raw)
  if not candidates or #candidates == 0 then return nil end
  if #candidates == 1 then return candidates[1] end
  if preferred_dimension then
    for _, unit in ipairs(candidates) do
      if unit.dimension == preferred_dimension then return unit end
    end
  end
  return candidates[1]
end

local function parse_number(raw)
  local value = tonumber(((raw or ""):gsub(",", "")))
  return value
end

-- Joined form: 10km / 72f. Degree/superscript chars allowed in unit tail.
local function split_number_unit(raw, preferred_dimension)
  local number, unit = raw:match("^([%+%-]?[%d%.,]+)([%a°/²]+)$")
  if not number then return nil end
  local value = parse_number(number)
  if not value then return nil end
  local unit_entry = lookup(unit, preferred_dimension)
  if not unit_entry then return nil end
  return value, unit_entry
end

local function parse_measure(text, preferred_dimension)
  text = text:match("^%s*(.-)%s*$")
  local number, unit = text:match("^([%+%-]?[%d%.,]+)%s+(%S+)$")
  if number then
    local value = parse_number(number)
    local unit_entry = lookup(unit, preferred_dimension)
    if value and unit_entry then return value, unit_entry end
    return nil
  end
  return split_number_unit(text, preferred_dimension)
end

local function convert_temperature(value, from_id, to_id)
  local kelvin
  if from_id == "°C" then kelvin = value + 273.15
  elseif from_id == "°F" then kelvin = (value - 32.0) * 5.0 / 9.0 + 273.15
  elseif from_id == "K" then kelvin = value
  else return nil end
  if to_id == "°C" then return kelvin - 273.15
  elseif to_id == "°F" then return (kelvin - 273.15) * 9.0 / 5.0 + 32.0
  elseif to_id == "K" then return kelvin
  else return nil end
end

local function convert(value, from_unit, to_unit)
  if from_unit.dimension ~= to_unit.dimension then return nil end
  if from_unit.dimension == "temperature" then
    return convert_temperature(value, from_unit.id, to_unit.id)
  end
  return value * from_unit.to_si / to_unit.to_si
end

local function format_number(value)
  if value == math.floor(value) and math.abs(value) < 1000000000000000 then
    return string.format("%.0f", value)
  end
  local formatted = string.format("%.6f", value)
  formatted = formatted:gsub("0+$", ""):gsub("%.$", "")
  return formatted
end

local function parse_forward(text)
  local left, to_symbol = text:match("^(.-)%s+[tT][oO]%s+(%S+)$")
  if not left then left, to_symbol = text:match("^(.-)%s+[iI][nN]%s+(%S+)$") end
  if not left then left, to_symbol = text:match("^(.-)%s+[aA][sS]%s+(%S+)$") end
  if not left then return nil end

  local value, from_unit = parse_measure(left, nil)
  if not value then return nil end
  if not lookup_all(to_symbol) then return nil end
  local to_unit = lookup(to_symbol, from_unit.dimension)
  if not to_unit then return nil end
  -- Re-parse from with to's dimension (10 oz to ml → floz).
  local from_again_v, from_again_u = parse_measure(left, to_unit.dimension)
  if from_again_v then
    value, from_unit = from_again_v, from_again_u
  end
  to_unit = lookup(to_symbol, from_unit.dimension)
  if not to_unit then return nil end
  return value, from_unit, to_unit
end

local function parse_reversed(text)
  local rest = text:match("^[hH][oO][wW]%s+[mM][aA][nN][yY]%s+(.+)$")
  if not rest then rest = text:match("^[wW][hH][aA][tT]%s+[iI][sS]%s+(.+)$") end
  if not rest then rest = text:match("^[wW][hH][aA][tT]%s*'?[sS]%s+(.+)$") end
  if not rest then return nil end

  local to_symbol, tail = rest:match("^(%S+)%s+[iI][nN]%s+(.+)$")
  if not to_symbol then return nil end

  tail = tail:match("^%s*(.-)%s*$")
  local bare_unit = tail:match("^[aA][nN]?%s+(%S+)$")
  local value, from_unit
  if bare_unit then
    from_unit = lookup(bare_unit, nil)
    if from_unit then value = 1 end
  else
    value, from_unit = parse_measure(tail, nil)
  end
  if not value or not from_unit then return nil end

  local to_unit = lookup(to_symbol, from_unit.dimension)
  if not to_unit then return nil end
  -- Re-resolve from with to's dimension (how many oz in a gal → floz).
  if bare_unit then
    from_unit = lookup(bare_unit, to_unit.dimension) or from_unit
  else
    local v2, f2 = parse_measure(tail, to_unit.dimension)
    if v2 then value, from_unit = v2, f2 end
  end
  to_unit = lookup(to_symbol, from_unit.dimension)
  if not to_unit then return nil end
  return value, from_unit, to_unit
end

return {
  id = "unit_converter",
  name = "Unit converter",
  description = 'Convert between units ("10 km to miles", "how many cups in a gallon").',
  api_version = 1,
  kind = "answerer",
  default_enabled = true,
  preference_section = "general",
  order = 20,
  capabilities = { "answers", "query" },
  pre_search_answers = function(query, ctx)
    if query.pageno > 1 then return nil end
    local text = strip_trailing_noise((query.query or ""):match("^%s*(.-)%s*$"))
    if text == "" then return nil end
    text = normalize_phrases(text)

    local value, from_unit, to_unit = parse_forward(text)
    if not value then
      value, from_unit, to_unit = parse_reversed(text)
    end
    if not value then return nil end

    local result = convert(value, from_unit, to_unit)
    if not result then return nil end
    return {
      answer = format_number(value) .. " " .. from_unit.id .. " = "
        .. format_number(result) .. " " .. to_unit.id,
      engine = "unit converter",
      interactive = {
        type = "unit",
        amount = value,
        from = from_unit.id,
        to = to_unit.id,
        result = result,
        dimension = from_unit.dimension,
      },
    }
  end,
}
