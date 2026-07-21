/** One-shot: rasterize Zoeken logo into PWA / social preview PNGs.
 *
 * Requires a one-time install: `bun add -d @resvg/resvg-js`
 * Then: `bun run generate-icons`
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const publicDir = join(root, "public");
const logoSvg = readFileSync(join(publicDir, "zoeken-logo.svg"), "utf8");

function rasterizeLogo(size) {
	const resvg = new Resvg(logoSvg, {
		fitTo: { mode: "width", value: size },
		background: "transparent",
	});
	return resvg.render().asPng();
}

function writePng(name, bytes) {
	writeFileSync(join(publicDir, name), bytes);
	console.log(`wrote public/${name} (${bytes.length} bytes)`);
}

for (const size of [180, 192, 512]) {
	const name = size === 180 ? "apple-touch-icon.png" : `icon-${size}.png`;
	writePng(name, rasterizeLogo(size));
}

const logoForOg = rasterizeLogo(360);
const logoDataUri = `data:image/png;base64,${Buffer.from(logoForOg).toString("base64")}`;

const ogSvg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="1200" height="630" viewBox="0 0 1200 630">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#e6ede1"/>
      <stop offset="1" stop-color="#f2f5ee"/>
    </linearGradient>
    <radialGradient id="glow" cx="50%" cy="35%" r="55%">
      <stop offset="0" stop-color="#246018" stop-opacity="0.16"/>
      <stop offset="1" stop-color="#246018" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1200" height="630" fill="url(#bg)"/>
  <rect width="1200" height="630" fill="url(#glow)"/>
  <image href="${logoDataUri}" xlink:href="${logoDataUri}" x="420" y="70" width="360" height="360"/>
  <text x="600" y="500" text-anchor="middle" font-family="Segoe UI, Helvetica, Arial, sans-serif"
    font-size="72" font-weight="700" fill="#0b140e">Zoeken</text>
  <text x="600" y="555" text-anchor="middle" font-family="Segoe UI, Helvetica, Arial, sans-serif"
    font-size="28" fill="#2a382e">Private metasearch</text>
</svg>`;

writePng(
	"og-image.png",
	new Resvg(ogSvg, { fitTo: { mode: "width", value: 1200 } }).render().asPng(),
);
