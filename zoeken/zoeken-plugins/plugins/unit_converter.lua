-- Self-contained unit table: length, mass, temperature, volume, speed,
-- data size, time, area. Deliberately not sourced from ctx.data.units (the
-- Wikidata unit dump): that table is noisy for everyday units — e.g. its
-- "gal" resolves to the CGS acceleration "galileo", not gallon, and it has
-- no "cup" at all. This curated table trades scientific-unit breadth for
-- correctness on the units people actually type.
local units_by_alias = {}

local function add_unit(aliases, display, dimension, factor)
  for _, alias in ipairs(aliases) do
    units_by_alias[alias] = { display = display, dimension = dimension, factor = factor }
  end
end

-- Length (base: meter)
add_unit({ "mm", "millimeter", "millimeters", "millimetre", "millimetres" }, "mm", "length", 0.001)
add_unit({ "cm", "centimeter", "centimeters", "centimetre", "centimetres" }, "cm", "length", 0.01)
add_unit({ "m", "meter", "meters", "metre", "metres" }, "m", "length", 1.0)
add_unit({ "km", "kilometer", "kilometers", "kilometre", "kilometres" }, "km", "length", 1000.0)
add_unit({ "in", "inch", "inches" }, "in", "length", 0.0254)
add_unit({ "ft", "foot", "feet" }, "ft", "length", 0.3048)
add_unit({ "yd", "yard", "yards" }, "yd", "length", 0.9144)
add_unit({ "mi", "mile", "miles" }, "mi", "length", 1609.344)
add_unit({ "nmi", "nautical-mile", "nauticalmiles" }, "nmi", "length", 1852.0)

-- Mass (base: kilogram)
add_unit({ "mg", "milligram", "milligrams" }, "mg", "mass", 1e-6)
add_unit({ "g", "gram", "grams" }, "g", "mass", 0.001)
add_unit({ "kg", "kilogram", "kilograms", "kilo", "kilos" }, "kg", "mass", 1.0)
add_unit({ "t", "ton", "tons", "tonne", "tonnes" }, "t", "mass", 1000.0)
add_unit({ "oz", "ounce", "ounces" }, "oz", "mass", 0.028349523125)
add_unit({ "lb", "lbs", "pound", "pounds" }, "lb", "mass", 0.45359237)
add_unit({ "st", "stone", "stones" }, "st", "mass", 6.35029318)

-- Temperature (special-cased below; factor unused)
add_unit({ "c", "celsius" }, "°C", "temperature", 1.0)
add_unit({ "f", "fahrenheit" }, "°F", "temperature", 1.0)
add_unit({ "k", "kelvin" }, "K", "temperature", 1.0)

-- Volume (base: liter)
add_unit({ "ml", "milliliter", "milliliters", "millilitre", "millilitres" }, "ml", "volume", 0.001)
add_unit({ "l", "liter", "liters", "litre", "litres" }, "l", "volume", 1.0)
add_unit({ "gal", "gallon", "gallons" }, "gal", "volume", 3.785411784)
add_unit({ "qt", "quart", "quarts" }, "qt", "volume", 0.946352946)
add_unit({ "pt", "pint", "pints" }, "pt", "volume", 0.473176473)
add_unit({ "cup", "cups" }, "cup", "volume", 0.2365882365)
add_unit({ "tbsp", "tablespoon", "tablespoons" }, "tbsp", "volume", 0.0147867648)
add_unit({ "tsp", "teaspoon", "teaspoons" }, "tsp", "volume", 0.00492892159)
add_unit({ "floz", "fl-oz" }, "fl oz", "volume", 0.0295735295625)

-- Speed (base: m/s)
add_unit({ "km/h", "kmh", "kph" }, "km/h", "speed", 1.0 / 3.6)
add_unit({ "mph" }, "mph", "speed", 0.44704)
add_unit({ "m/s", "ms" }, "m/s", "speed", 1.0)
add_unit({ "knot", "knots", "kn" }, "kn", "speed", 0.514444444)

-- Data (base: byte; decimal SI + binary)
add_unit({ "bit", "bits" }, "bit", "data", 0.125)
add_unit({ "b", "byte", "bytes" }, "B", "data", 1.0)
add_unit({ "kb", "kilobyte", "kilobytes" }, "kB", "data", 1e3)
add_unit({ "mb", "megabyte", "megabytes" }, "MB", "data", 1e6)
add_unit({ "gb", "gigabyte", "gigabytes" }, "GB", "data", 1e9)
add_unit({ "tb", "terabyte", "terabytes" }, "TB", "data", 1e12)
add_unit({ "kib", "kibibyte", "kibibytes" }, "KiB", "data", 1024.0)
add_unit({ "mib", "mebibyte", "mebibytes" }, "MiB", "data", 1048576.0)
add_unit({ "gib", "gibibyte", "gibibytes" }, "GiB", "data", 1073741824.0)
add_unit({ "tib", "tebibyte", "tebibytes" }, "TiB", "data", 1099511627776.0)

-- Time (base: second)
add_unit({ "s", "sec", "secs", "second", "seconds" }, "s", "time", 1.0)
add_unit({ "min", "mins", "minute", "minutes" }, "min", "time", 60.0)
add_unit({ "h", "hr", "hrs", "hour", "hours" }, "h", "time", 3600.0)
add_unit({ "day", "days" }, "days", "time", 86400.0)
add_unit({ "week", "weeks" }, "weeks", "time", 604800.0)

-- Area (base: square meter)
add_unit({ "sqm", "m2", "m²" }, "m²", "area", 1.0)
add_unit({ "sqft", "ft2", "ft²" }, "ft²", "area", 0.09290304)
add_unit({ "acre", "acres" }, "acres", "area", 4046.8564224)
add_unit({ "ha", "hectare", "hectares" }, "ha", "area", 10000.0)
add_unit({ "km2", "km²", "sqkm" }, "km²", "area", 1e6)

-- Trailing words that don't change the request ("10 km to miles please?").
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

local function lookup(raw)
  return units_by_alias[(raw or ""):lower()]
end

local function parse_number(raw)
  -- Extra parens truncate gsub's 2nd return (replacement count) — otherwise
  -- tonumber(s, count) treats it as a base and errors when count is 0.
  local value = tonumber(((raw or ""):gsub(",", "")))
  return value
end

-- `10km` / `72f` (joined, no space).
local function split_number_unit(raw)
  local number, unit = raw:match("^([%+%-]?[%d%.,]+)(%a[%a°/²]*)$")
  if not number then return nil end
  local value = parse_number(number)
  if not value then return nil end
  local unit_entry = lookup(unit)
  if not unit_entry then return nil end
  return value, unit_entry
end

local function parse_measure(text)
  text = text:match("^%s*(.-)%s*$")
  local number, unit = text:match("^([%+%-]?[%d%.,]+)%s+(%S+)$")
  if number then
    local value = parse_number(number)
    local unit_entry = lookup(unit)
    if value and unit_entry then return value, unit_entry end
    return nil
  end
  return split_number_unit(text)
end

local function convert_temperature(value, from_display, to_display)
  local kelvin
  if from_display == "°C" then kelvin = value + 273.15
  elseif from_display == "°F" then kelvin = (value - 32.0) * 5.0 / 9.0 + 273.15
  elseif from_display == "K" then kelvin = value
  else return nil end
  if to_display == "°C" then return kelvin - 273.15
  elseif to_display == "°F" then return (kelvin - 273.15) * 9.0 / 5.0 + 32.0
  elseif to_display == "K" then return kelvin
  else return nil end
end

local function convert(value, from_unit, to_unit)
  if from_unit.dimension ~= to_unit.dimension then return nil end
  if from_unit.dimension == "temperature" then
    return convert_temperature(value, from_unit.display, to_unit.display)
  end
  return value * from_unit.factor / to_unit.factor
end

local function format_number(value)
  if value == math.floor(value) and math.abs(value) < 1000000000000000 then
    return string.format("%.0f", value)
  end
  local formatted = string.format("%.6f", value)
  formatted = formatted:gsub("0+$", ""):gsub("%.$", "")
  return formatted
end

-- Forward form: "<value> <unit> to|in|as <unit>" (also "<value><unit> to <unit>").
local function parse_forward(text)
  local left, to_symbol = text:match("^(.-)%s+[tT][oO]%s+(%S+)$")
  if not left then left, to_symbol = text:match("^(.-)%s+[iI][nN]%s+(%S+)$") end
  if not left then left, to_symbol = text:match("^(.-)%s+[aA][sS]%s+(%S+)$") end
  if not left then return nil end
  local to_unit = lookup(to_symbol)
  if not to_unit then return nil end
  local value, from_unit = parse_measure(left)
  if not value then return nil end
  return value, from_unit, to_unit
end

-- Reversed/natural-language form: "how many <unit> in <value> <unit>",
-- "how many cups in a gallon" (implicit value 1 with "a"/"an").
local function parse_reversed(text)
  local rest = text:match("^[hH][oO][wW]%s+[mM][aA][nN][yY]%s+(.+)$")
  if not rest then rest = text:match("^[wW][hH][aA][tT]%s+[iI][sS]%s+(.+)$") end
  if not rest then rest = text:match("^[wW][hH][aA][tT]%s*'?[sS]%s+(.+)$") end
  if not rest then return nil end

  local to_symbol, tail = rest:match("^(%S+)%s+[iI][nN]%s+(.+)$")
  if not to_symbol then return nil end
  local to_unit = lookup(to_symbol)
  if not to_unit then return nil end

  tail = tail:match("^%s*(.-)%s*$")
  local bare_unit = tail:match("^[aA][nN]?%s+(%S+)$")
  if bare_unit then
    local from_unit = lookup(bare_unit)
    if from_unit then return 1, from_unit, to_unit end
  end
  local value, from_unit = parse_measure(tail)
  if not value then return nil end
  return value, from_unit, to_unit
end

return {
  id = "unit_converter",
  name = "Unit converter",
  description = "Convert between units (\"10 km to miles\", \"how many cups in a gallon\").",
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

    local value, from_unit, to_unit = parse_forward(text)
    if not value then
      value, from_unit, to_unit = parse_reversed(text)
    end
    if not value then return nil end

    local result = convert(value, from_unit, to_unit)
    if not result then return nil end
    return {
      answer = format_number(value) .. " " .. from_unit.display .. " = "
        .. format_number(result) .. " " .. to_unit.display,
      engine = "unit converter",
    }
  end,
}
