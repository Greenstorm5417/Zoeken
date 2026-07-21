-- Curated everyday units (from zoeken-client/src/lib/units.json).
-- Not Wikidata: that dump maps "gal" to galileo and has no "cup".
-- Regenerate: bun zoeken/zoeken-plugins/plugins/gen_units_lua.mjs

local UNITS = {
  { id = 'mm', name = 'millimeter', dimension = 'length', si_unit = 'm', to_si = 0.001, abbreviations = { 'mm', 'millimeter', 'millimeters', 'millimetre', 'millimetres' } },
  { id = 'cm', name = 'centimeter', dimension = 'length', si_unit = 'm', to_si = 0.01, abbreviations = { 'cm', 'centimeter', 'centimeters', 'centimetre', 'centimetres' } },
  { id = 'm', name = 'meter', dimension = 'length', si_unit = 'm', to_si = 1, abbreviations = { 'm', 'meter', 'meters', 'metre', 'metres' } },
  { id = 'km', name = 'kilometer', dimension = 'length', si_unit = 'm', to_si = 1000, abbreviations = { 'km', 'kilometer', 'kilometers', 'kilometre', 'kilometres' } },
  { id = 'in', name = 'inch', dimension = 'length', si_unit = 'm', to_si = 0.0254, abbreviations = { 'in', 'inch', 'inches' } },
  { id = 'ft', name = 'foot', dimension = 'length', si_unit = 'm', to_si = 0.3048, abbreviations = { 'ft', 'foot', 'feet' } },
  { id = 'yd', name = 'yard', dimension = 'length', si_unit = 'm', to_si = 0.9144, abbreviations = { 'yd', 'yard', 'yards' } },
  { id = 'mi', name = 'mile', dimension = 'length', si_unit = 'm', to_si = 1609.344, abbreviations = { 'mi', 'mile', 'miles' } },
  { id = 'nmi', name = 'nautical mile', dimension = 'length', si_unit = 'm', to_si = 1852, abbreviations = { 'nmi', 'nautical mile', 'nautical miles', 'nautical-mile', 'nauticalmiles' } },
  { id = 'mg', name = 'milligram', dimension = 'mass', si_unit = 'kg', to_si = 0.000001, abbreviations = { 'mg', 'milligram', 'milligrams' } },
  { id = 'g', name = 'gram', dimension = 'mass', si_unit = 'kg', to_si = 0.001, abbreviations = { 'g', 'gram', 'grams' } },
  { id = 'kg', name = 'kilogram', dimension = 'mass', si_unit = 'kg', to_si = 1, abbreviations = { 'kg', 'kilogram', 'kilograms', 'kilo', 'kilos' } },
  { id = 't', name = 'tonne', dimension = 'mass', si_unit = 'kg', to_si = 1000, abbreviations = { 't', 'tonne', 'tonnes', 'metric ton', 'metric tons' } },
  { id = 'oz', name = 'ounce', dimension = 'mass', si_unit = 'kg', to_si = 0.028349523125, abbreviations = { 'oz', 'ounce', 'ounces' } },
  { id = 'lb', name = 'pound', dimension = 'mass', si_unit = 'kg', to_si = 0.45359237, abbreviations = { 'lb', 'lbs', 'pound', 'pounds' } },
  { id = 'st', name = 'stone', dimension = 'mass', si_unit = 'kg', to_si = 6.35029318, abbreviations = { 'st', 'stone', 'stones' } },
  { id = 'ton', name = 'short ton', dimension = 'mass', si_unit = 'kg', to_si = 907.18474, abbreviations = { 'ton', 'tons', 'short ton', 'short tons', 'us ton' } },
  { id = '°C', name = 'Celsius', dimension = 'temperature', si_unit = 'K', to_si = 1, abbreviations = { 'c', '°c', 'celsius', 'centigrade' } },
  { id = '°F', name = 'Fahrenheit', dimension = 'temperature', si_unit = 'K', to_si = 1, abbreviations = { 'f', '°f', 'fahrenheit' } },
  { id = 'K', name = 'Kelvin', dimension = 'temperature', si_unit = 'K', to_si = 1, abbreviations = { 'k', 'kelvin', 'kelvins' } },
  { id = 'ml', name = 'milliliter', dimension = 'volume', si_unit = 'm³', to_si = 0.000001, abbreviations = { 'ml', 'milliliter', 'milliliters', 'millilitre', 'millilitres', 'cc' } },
  { id = 'l', name = 'liter', dimension = 'volume', si_unit = 'm³', to_si = 0.001, abbreviations = { 'l', 'liter', 'liters', 'litre', 'litres' } },
  { id = 'm³', name = 'cubic meter', dimension = 'volume', si_unit = 'm³', to_si = 1, abbreviations = { 'm3', 'm³', 'cubic meter', 'cubic meters', 'cubic metre', 'cubic metres' } },
  { id = 'gal', name = 'US gallon', dimension = 'volume', si_unit = 'm³', to_si = 0.003785411784, abbreviations = { 'gal', 'gallon', 'gallons', 'us gal', 'us gallon', 'us gallons' } },
  { id = 'ukgal', name = 'imperial gallon', dimension = 'volume', si_unit = 'm³', to_si = 0.00454609, abbreviations = { 'ukgal', 'uk gal', 'imperial gallon', 'imperial gallons', 'imp gal', 'imperial gal' } },
  { id = 'qt', name = 'quart', dimension = 'volume', si_unit = 'm³', to_si = 0.000946352946, abbreviations = { 'qt', 'quart', 'quarts' } },
  { id = 'pt', name = 'pint', dimension = 'volume', si_unit = 'm³', to_si = 0.000473176473, abbreviations = { 'pt', 'pint', 'pints' } },
  { id = 'cup', name = 'cup', dimension = 'volume', si_unit = 'm³', to_si = 0.0002365882365, abbreviations = { 'cup', 'cups' } },
  { id = 'tbsp', name = 'tablespoon', dimension = 'volume', si_unit = 'm³', to_si = 0.0000147867648, abbreviations = { 'tbsp', 'tablespoon', 'tablespoons', 'tbs' } },
  { id = 'tsp', name = 'teaspoon', dimension = 'volume', si_unit = 'm³', to_si = 0.00000492892159, abbreviations = { 'tsp', 'teaspoon', 'teaspoons' } },
  { id = 'floz', name = 'fluid ounce', dimension = 'volume', si_unit = 'm³', to_si = 0.0000295735295625, abbreviations = { 'floz', 'fl oz', 'fl-oz', 'fluid ounce', 'fluid ounces', 'oz' } },
  { id = 'm/s', name = 'meters per second', dimension = 'speed', si_unit = 'm/s', to_si = 1, abbreviations = { 'm/s', 'mps', 'meters per second', 'metres per second' } },
  { id = 'km/h', name = 'kilometers per hour', dimension = 'speed', si_unit = 'm/s', to_si = 0.2777777777777778, abbreviations = { 'km/h', 'kmh', 'kph', 'kilometers per hour', 'kilometres per hour' } },
  { id = 'mph', name = 'miles per hour', dimension = 'speed', si_unit = 'm/s', to_si = 0.44704, abbreviations = { 'mph', 'miles per hour' } },
  { id = 'kn', name = 'knot', dimension = 'speed', si_unit = 'm/s', to_si = 0.514444444, abbreviations = { 'kn', 'kt', 'knot', 'knots' } },
  { id = 'ft/s', name = 'feet per second', dimension = 'speed', si_unit = 'm/s', to_si = 0.3048, abbreviations = { 'ft/s', 'fps', 'feet per second' } },
  { id = 'bit', name = 'bit', dimension = 'data', si_unit = 'B', to_si = 0.125, abbreviations = { 'bit', 'bits' } },
  { id = 'B', name = 'byte', dimension = 'data', si_unit = 'B', to_si = 1, abbreviations = { 'b', 'byte', 'bytes' } },
  { id = 'kB', name = 'kilobyte', dimension = 'data', si_unit = 'B', to_si = 1000, abbreviations = { 'kb', 'kilobyte', 'kilobytes' } },
  { id = 'MB', name = 'megabyte', dimension = 'data', si_unit = 'B', to_si = 1000000, abbreviations = { 'mb', 'megabyte', 'megabytes' } },
  { id = 'GB', name = 'gigabyte', dimension = 'data', si_unit = 'B', to_si = 1000000000, abbreviations = { 'gb', 'gigabyte', 'gigabytes' } },
  { id = 'TB', name = 'terabyte', dimension = 'data', si_unit = 'B', to_si = 1000000000000, abbreviations = { 'tb', 'terabyte', 'terabytes' } },
  { id = 'PB', name = 'petabyte', dimension = 'data', si_unit = 'B', to_si = 1000000000000000, abbreviations = { 'pb', 'petabyte', 'petabytes' } },
  { id = 'KiB', name = 'kibibyte', dimension = 'data', si_unit = 'B', to_si = 1024, abbreviations = { 'kib', 'kibibyte', 'kibibytes' } },
  { id = 'MiB', name = 'mebibyte', dimension = 'data', si_unit = 'B', to_si = 1048576, abbreviations = { 'mib', 'mebibyte', 'mebibytes' } },
  { id = 'GiB', name = 'gibibyte', dimension = 'data', si_unit = 'B', to_si = 1073741824, abbreviations = { 'gib', 'gibibyte', 'gibibytes' } },
  { id = 'TiB', name = 'tebibyte', dimension = 'data', si_unit = 'B', to_si = 1099511627776, abbreviations = { 'tib', 'tebibyte', 'tebibytes' } },
  { id = 'PiB', name = 'pebibyte', dimension = 'data', si_unit = 'B', to_si = 1125899906842624, abbreviations = { 'pib', 'pebibyte', 'pebibytes' } },
  { id = 'ms', name = 'millisecond', dimension = 'time', si_unit = 's', to_si = 0.001, abbreviations = { 'ms', 'millisecond', 'milliseconds', 'msec' } },
  { id = 's', name = 'second', dimension = 'time', si_unit = 's', to_si = 1, abbreviations = { 's', 'sec', 'secs', 'second', 'seconds' } },
  { id = 'min', name = 'minute', dimension = 'time', si_unit = 's', to_si = 60, abbreviations = { 'min', 'mins', 'minute', 'minutes' } },
  { id = 'h', name = 'hour', dimension = 'time', si_unit = 's', to_si = 3600, abbreviations = { 'h', 'hr', 'hrs', 'hour', 'hours' } },
  { id = 'days', name = 'day', dimension = 'time', si_unit = 's', to_si = 86400, abbreviations = { 'day', 'days', 'd' } },
  { id = 'weeks', name = 'week', dimension = 'time', si_unit = 's', to_si = 604800, abbreviations = { 'week', 'weeks', 'wk', 'wks' } },
  { id = 'mm²', name = 'square millimeter', dimension = 'area', si_unit = 'm²', to_si = 0.000001, abbreviations = { 'mm2', 'mm²', 'sqmm', 'square millimeter', 'square millimeters' } },
  { id = 'cm²', name = 'square centimeter', dimension = 'area', si_unit = 'm²', to_si = 0.0001, abbreviations = { 'cm2', 'cm²', 'sqcm', 'square centimeter', 'square centimeters' } },
  { id = 'm²', name = 'square meter', dimension = 'area', si_unit = 'm²', to_si = 1, abbreviations = { 'm2', 'm²', 'sqm', 'square meter', 'square meters', 'square metre', 'square metres' } },
  { id = 'km²', name = 'square kilometer', dimension = 'area', si_unit = 'm²', to_si = 1000000, abbreviations = { 'km2', 'km²', 'sqkm', 'square kilometer', 'square kilometers' } },
  { id = 'in²', name = 'square inch', dimension = 'area', si_unit = 'm²', to_si = 0.00064516, abbreviations = { 'in2', 'in²', 'sqin', 'square inch', 'square inches' } },
  { id = 'ft²', name = 'square foot', dimension = 'area', si_unit = 'm²', to_si = 0.09290304, abbreviations = { 'ft2', 'ft²', 'sqft', 'square foot', 'square feet' } },
  { id = 'yd²', name = 'square yard', dimension = 'area', si_unit = 'm²', to_si = 0.83612736, abbreviations = { 'yd2', 'yd²', 'sqyd', 'square yard', 'square yards' } },
  { id = 'acres', name = 'acre', dimension = 'area', si_unit = 'm²', to_si = 4046.8564224, abbreviations = { 'acre', 'acres' } },
  { id = 'ha', name = 'hectare', dimension = 'area', si_unit = 'm²', to_si = 10000, abbreviations = { 'ha', 'hectare', 'hectares' } },
  { id = 'mi²', name = 'square mile', dimension = 'area', si_unit = 'm²', to_si = 2589988.110336, abbreviations = { 'mi2', 'mi²', 'sqmi', 'square mile', 'square miles' } },
  { id = 'Pa', name = 'pascal', dimension = 'pressure', si_unit = 'Pa', to_si = 1, abbreviations = { 'pa', 'pascal', 'pascals' } },
  { id = 'kPa', name = 'kilopascal', dimension = 'pressure', si_unit = 'Pa', to_si = 1000, abbreviations = { 'kpa', 'kilopascal', 'kilopascals' } },
  { id = 'bar', name = 'bar', dimension = 'pressure', si_unit = 'Pa', to_si = 100000, abbreviations = { 'bar', 'bars' } },
  { id = 'atm', name = 'atmosphere', dimension = 'pressure', si_unit = 'Pa', to_si = 101325, abbreviations = { 'atm', 'atmosphere', 'atmospheres' } },
  { id = 'psi', name = 'pound per square inch', dimension = 'pressure', si_unit = 'Pa', to_si = 6894.757293168, abbreviations = { 'psi', 'lbf/in2', 'pounds per square inch' } },
  { id = 'mmHg', name = 'millimeter of mercury', dimension = 'pressure', si_unit = 'Pa', to_si = 133.322387415, abbreviations = { 'mmhg', 'torr', 'millimeter of mercury', 'millimetre of mercury' } },
  { id = 'J', name = 'joule', dimension = 'energy', si_unit = 'J', to_si = 1, abbreviations = { 'j', 'joule', 'joules' } },
  { id = 'kJ', name = 'kilojoule', dimension = 'energy', si_unit = 'J', to_si = 1000, abbreviations = { 'kj', 'kilojoule', 'kilojoules' } },
  { id = 'cal', name = 'calorie', dimension = 'energy', si_unit = 'J', to_si = 4.184, abbreviations = { 'cal', 'calorie', 'calories' } },
  { id = 'kcal', name = 'kilocalorie', dimension = 'energy', si_unit = 'J', to_si = 4184, abbreviations = { 'kcal', 'kilocalorie', 'kilocalories', 'Cal', 'Calories' } },
  { id = 'Wh', name = 'watt-hour', dimension = 'energy', si_unit = 'J', to_si = 3600, abbreviations = { 'wh', 'watt-hour', 'watt-hours', 'watt hour', 'watt hours' } },
  { id = 'kWh', name = 'kilowatt-hour', dimension = 'energy', si_unit = 'J', to_si = 3600000, abbreviations = { 'kwh', 'kilowatt-hour', 'kilowatt-hours', 'kilowatt hour', 'kilowatt hours' } },
  { id = 'BTU', name = 'British thermal unit', dimension = 'energy', si_unit = 'J', to_si = 1055.05585262, abbreviations = { 'btu', 'btus', 'british thermal unit', 'british thermal units' } },
  { id = 'W', name = 'watt', dimension = 'power', si_unit = 'W', to_si = 1, abbreviations = { 'w', 'watt', 'watts' } },
  { id = 'kW', name = 'kilowatt', dimension = 'power', si_unit = 'W', to_si = 1000, abbreviations = { 'kw', 'kilowatt', 'kilowatts' } },
  { id = 'MW', name = 'megawatt', dimension = 'power', si_unit = 'W', to_si = 1000000, abbreviations = { 'mw', 'megawatt', 'megawatts' } },
  { id = 'hp', name = 'horsepower', dimension = 'power', si_unit = 'W', to_si = 745.6998715822702, abbreviations = { 'hp', 'horsepower', 'bhp' } },
  { id = 'N', name = 'newton', dimension = 'force', si_unit = 'N', to_si = 1, abbreviations = { 'n', 'newton', 'newtons' } },
  { id = 'kN', name = 'kilonewton', dimension = 'force', si_unit = 'N', to_si = 1000, abbreviations = { 'kilonewton', 'kilonewtons' } },
  { id = 'lbf', name = 'pound-force', dimension = 'force', si_unit = 'N', to_si = 4.4482216152605, abbreviations = { 'lbf', 'pound-force', 'pounds-force', 'lb force' } },
  { id = 'kgf', name = 'kilogram-force', dimension = 'force', si_unit = 'N', to_si = 9.80665, abbreviations = { 'kgf', 'kilogram-force', 'kilopond', 'kp' } },
  { id = 'rad', name = 'radian', dimension = 'angle', si_unit = 'rad', to_si = 1, abbreviations = { 'rad', 'radian', 'radians' } },
  { id = 'deg', name = 'degree', dimension = 'angle', si_unit = 'rad', to_si = 0.017453292519943295, abbreviations = { 'deg', 'degree', 'degrees', '°' } },
  { id = 'grad', name = 'gradian', dimension = 'angle', si_unit = 'rad', to_si = 0.015707963267948967, abbreviations = { 'grad', 'gradian', 'gradians', 'gon' } },
  { id = 'arcmin', name = 'arcminute', dimension = 'angle', si_unit = 'rad', to_si = 0.0002908882086657216, abbreviations = { 'arcmin', 'arcminute', 'arcminutes' } },
  { id = 'arcsec', name = 'arcsecond', dimension = 'angle', si_unit = 'rad', to_si = 0.00000484813681109536, abbreviations = { 'arcsec', 'arcsecond', 'arcseconds' } },
  { id = 'turn', name = 'turn', dimension = 'angle', si_unit = 'rad', to_si = 6.283185307179586, abbreviations = { 'turn', 'turns', 'rev', 'revolution', 'revolutions' } },
}

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
