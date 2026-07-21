import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const jsonPath = path.resolve(
	here,
	"../../../zoeken-client/src/lib/units.json",
);
const data = JSON.parse(fs.readFileSync(jsonPath, "utf8"));

function esc(s) {
	return String(s).replace(/\\/g, "\\\\").replace(/'/g, "\\'");
}

const unitLines = [
	"-- Curated everyday units (from zoeken-client/src/lib/units.json).",
	'-- Not Wikidata: that dump maps "gal" to galileo and has no "cup".',
	"-- Regenerate: bun zoeken/zoeken-plugins/plugins/gen_units_lua.mjs",
	"",
	"local UNITS = {",
];
for (const u of data.units) {
	const abs = u.abbreviations.map((a) => `'${esc(a)}'`).join(", ");
	unitLines.push(
		`  { id = '${esc(u.id)}', name = '${esc(u.name)}', dimension = '${esc(u.dimension)}', si_unit = '${esc(u.si_unit)}', to_si = ${JSON.stringify(u.to_si)}, abbreviations = { ${abs} } },`,
	);
}
unitLines.push("}");

const logicPath = path.join(here, "unit_converter_logic.lua");
const logic = fs.readFileSync(logicPath, "utf8");
const out = `${unitLines.join("\n")}\n\n${logic.trimStart()}`;
fs.writeFileSync(path.join(here, "unit_converter.lua"), out.endsWith("\n") ? out : `${out}\n`);
console.log(`wrote unit_converter.lua (${data.units.length} units)`);
