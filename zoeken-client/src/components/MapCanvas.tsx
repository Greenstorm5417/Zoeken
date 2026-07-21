import { useEffect, useRef } from "react";
import type { SearchResult } from "#/lib/api";

export type MapPoint = {
	lat: number;
	lon: number;
	title: string;
	url: string;
};

/** Pull mlat/mlon (or lat/lon) query params from a result URL. */
export function coordsFromResult(result: SearchResult): MapPoint | null {
	try {
		const u = new URL(result.url);
		const lat = Number(
			u.searchParams.get("mlat") ?? u.searchParams.get("lat") ?? "",
		);
		const lon = Number(
			u.searchParams.get("mlon") ?? u.searchParams.get("lon") ?? "",
		);
		if (!Number.isFinite(lat) || !Number.isFinite(lon)) return null;
		return { lat, lon, title: result.title, url: result.url };
	} catch {
		return null;
	}
}

type LeafletMap = {
	remove: () => void;
	fitBounds: (bounds: unknown, opts?: { padding?: [number, number] }) => void;
	setView: (center: [number, number], zoom: number) => void;
};

type LeafletNs = {
	map: (el: HTMLElement) => LeafletMap & {
		addLayer: (layer: unknown) => void;
	};
	tileLayer: (
		url: string,
		opts: Record<string, unknown>,
	) => { addTo: (map: unknown) => void };
	marker: (latlng: [number, number]) => {
		addTo: (map: unknown) => {
			bindPopup: (html: string) => void;
		};
	};
	latLngBounds: (points: Array<[number, number]>) => unknown;
};

declare global {
	interface Window {
		L?: LeafletNs;
	}
}

let leafletPromise: Promise<LeafletNs> | null = null;

function loadLeaflet(): Promise<LeafletNs> {
	if (window.L) return Promise.resolve(window.L);
	if (leafletPromise) return leafletPromise;
	leafletPromise = new Promise((resolve, reject) => {
		const cssId = "leaflet-cdn-css";
		if (!document.getElementById(cssId)) {
			const link = document.createElement("link");
			link.id = cssId;
			link.rel = "stylesheet";
			link.href = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.css";
			document.head.appendChild(link);
		}
		const existing = document.getElementById("leaflet-cdn-js");
		if (existing) {
			existing.addEventListener("load", () => {
				if (window.L) resolve(window.L);
				else reject(new Error("Leaflet failed to load"));
			});
			return;
		}
		const script = document.createElement("script");
		script.id = "leaflet-cdn-js";
		script.src = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.js";
		script.async = true;
		script.onload = () => {
			if (window.L) resolve(window.L);
			else reject(new Error("Leaflet failed to load"));
		};
		script.onerror = () => reject(new Error("Leaflet script error"));
		document.head.appendChild(script);
	});
	return leafletPromise;
}

/** One interactive OSM map with markers from geocoded search results. */
export function MapCanvas({ points }: { points: MapPoint[] }) {
	const containerRef = useRef<HTMLDivElement | null>(null);
	const pointsRef = useRef(points);
	pointsRef.current = points;
	// Stable key so we don't remount Leaflet on every parent render.
	const pointsKey = points.map((p) => `${p.lat},${p.lon},${p.url}`).join("|");

	// biome-ignore lint/correctness/useExhaustiveDependencies: pointsKey triggers remount when markers change; body reads pointsRef
	useEffect(() => {
		const pts = pointsRef.current;
		if (!pts.length || !containerRef.current) return;
		let cancelled = false;
		let map: LeafletMap | null = null;

		void loadLeaflet().then((L) => {
			if (cancelled || !containerRef.current) return;
			map = L.map(containerRef.current);
			L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
				attribution:
					'&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
				maxZoom: 19,
			}).addTo(map);

			const latlngs: Array<[number, number]> = [];
			for (const point of pts) {
				latlngs.push([point.lat, point.lon]);
				const marker = L.marker([point.lat, point.lon]).addTo(map);
				marker.bindPopup(
					`<a href="${point.url.replace(/"/g, "&quot;")}" target="_blank" rel="noopener noreferrer">${point.title.replace(/</g, "&lt;")}</a>`,
				);
			}
			if (latlngs.length === 1) {
				map.setView(latlngs[0], 14);
			} else {
				map.fitBounds(L.latLngBounds(latlngs), { padding: [24, 24] });
			}
		});

		return () => {
			cancelled = true;
			map?.remove();
		};
	}, [pointsKey]);

	if (!points.length) return null;

	return (
		<div
			ref={containerRef}
			className="mb-6 h-72 w-full max-w-5xl overflow-hidden rounded-xl border border-line"
			role="img"
			aria-label="Map of search results"
		/>
	);
}
