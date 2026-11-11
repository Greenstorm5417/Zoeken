import type { SearchResult } from "./api";

export type MapPoint = {
	lat: number;
	lon: number;
	title: string;
	url: string;
};

/** Read coordinates from structured fields, then from the URL. Blank is not zero. */
export function coordsFromResult(result: SearchResult): MapPoint | null {
	const structured = structuredCoords(result);
	const fromUrl = structured ?? urlCoords(result.url);
	if (!fromUrl) return null;
	if (!validLatLon(fromUrl.lat, fromUrl.lon)) return null;
	return {
		lat: fromUrl.lat,
		lon: fromUrl.lon,
		title: result.title,
		url: result.url,
	};
}

export function parseCoord(raw: string | null | undefined): number | null {
	if (raw == null) return null;
	const trimmed = raw.trim();
	if (!trimmed) return null;
	const value = Number(trimmed);
	return Number.isFinite(value) ? value : null;
}

export function validLatLon(lat: number, lon: number): boolean {
	return lat >= -90 && lat <= 90 && lon >= -180 && lon <= 180;
}

function structuredCoords(
	result: SearchResult,
): { lat: number; lon: number } | null {
	if (result.kind !== "key_value") return null;
	const map = new Map(
		result.kvmap.map(([key, value]) => [key.toLowerCase(), value]),
	);
	const lat = parseCoord(
		map.get("lat") ?? map.get("latitude") ?? map.get("mlat"),
	);
	const lon = parseCoord(
		map.get("lon") ?? map.get("lng") ?? map.get("longitude") ?? map.get("mlon"),
	);
	if (lat == null || lon == null) return null;
	return { lat, lon };
}

function urlCoords(url: string): { lat: number; lon: number } | null {
	try {
		const parsed = new URL(url);
		const lat = parseCoord(
			parsed.searchParams.get("mlat") ?? parsed.searchParams.get("lat"),
		);
		const lon = parseCoord(
			parsed.searchParams.get("mlon") ?? parsed.searchParams.get("lon"),
		);
		if (lat == null || lon == null) return null;
		return { lat, lon };
	} catch {
		return null;
	}
}
