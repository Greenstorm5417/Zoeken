import { lazy, Suspense, useRef, useState } from "react";
import {
	MapResult,
	ResultItem,
	specializedTemplate,
} from "#/components/ResultTemplates";
import { VideoCard } from "#/components/VideoCard";
import type { SearchResult } from "#/lib/api";
import { resultImgSrc, resultThumbnail } from "#/lib/api";
import { coordsFromResult } from "#/lib/coords";
import { activeStrings } from "#/lib/i18n";
import { engineNames } from "#/lib/searchDisplay";

const ImageLightbox = lazy(() =>
	import("#/components/ImageLightbox").then((m) => ({
		default: m.ImageLightbox,
	})),
);
const MapCanvas = lazy(() =>
	import("#/components/MapCanvas").then((m) => ({ default: m.MapCanvas })),
);

type Props = {
	results: SearchResult[];
	activeCategory: string;
	videoMode: boolean;
	imageMode: boolean;
	mapMode: boolean;
	newTab?: boolean;
	urlFormatting?: string;
	cacheUrl?: string;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
	empty: boolean;
	emptyTitle: string;
	emptyHint: string;
};

/** Category-aware result list (video / image / map / general). */
export function SearchResultList({
	results,
	activeCategory,
	videoMode,
	imageMode,
	mapMode,
	newTab,
	urlFormatting,
	cacheUrl,
	query = "",
	onHideSite,
	onPreferSite,
	empty,
	emptyTitle,
	emptyHint,
}: Props) {
	const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
	const openerRef = useRef<HTMLButtonElement | null>(null);

	if (empty) {
		return (
			<div className="max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
				<p className="font-medium text-ink">{emptyTitle}</p>
				<p className="mt-1 text-sm text-ink-muted">{emptyHint}</p>
			</div>
		);
	}
	if (results.length === 0) return null;

	if (videoMode) {
		return (
			<ul className="grid max-w-5xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{results.map((result) => (
					<li key={result.url} className="zoeken-result">
						<VideoCard result={result} newTab={newTab} />
					</li>
				))}
			</ul>
		);
	}

	if (imageMode) {
		const images = results.filter((r) => resultThumbnail(r) || resultImgSrc(r));
		return (
			<>
				<ul className="grid max-w-5xl grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
					{images.map((result, index) => {
						const ratio = imageRatio(result);
						return (
							<li key={result.url} className="zoeken-result">
								<button
									type="button"
									onClick={(event) => {
										openerRef.current = event.currentTarget;
										setLightboxIndex(index);
									}}
									className="group block w-full overflow-hidden rounded-xl text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
								>
									<img
										src={resultThumbnail(result) || resultImgSrc(result)}
										alt={result.title || ""}
										style={ratio ? { aspectRatio: ratio } : undefined}
										className="max-h-56 w-full bg-surface-raised object-contain"
										decoding="async"
										loading={index < 8 ? "eager" : "lazy"}
										fetchPriority={index < 2 ? "high" : undefined}
										onError={(event) => {
											event.currentTarget.replaceWith(imageFallback());
										}}
									/>
									<p className="mt-1.5 truncate text-xs text-ink-muted group-hover:text-accent">
										{result.title || "Image"}
									</p>
									<p className="truncate text-[0.65rem] text-ink-subtle">
										{result.kind === "image" && result.source
											? result.source
											: ""}
										{result.kind === "image" &&
										result.resolution &&
										result.resolution !== "unknown"
											? `${result.source ? " · " : ""}${result.resolution}`
											: ""}
									</p>
								</button>
								{onHideSite ? (
									<button
										type="button"
										className="mt-1 min-h-11 text-xs text-ink-muted hover:text-accent"
										onClick={() => {
											try {
												const host = new URL(result.url).hostname.replace(
													/^www\./,
													"",
												);
												if (host) onHideSite(host);
											} catch {
												/* ignore unparseable image urls */
											}
										}}
									>
										{activeStrings().hideThisSite}
									</button>
								) : null}
							</li>
						);
					})}
				</ul>
				{lightboxIndex != null ? (
					<Suspense fallback={null}>
						<ImageLightbox
							results={images}
							index={lightboxIndex}
							onIndex={setLightboxIndex}
							onClose={() => {
								setLightboxIndex(null);
								openerRef.current?.focus();
							}}
						/>
					</Suspense>
				) : null}
			</>
		);
	}

	if (mapMode) {
		const points = results
			.map(coordsFromResult)
			.filter((p): p is NonNullable<typeof p> => p != null);
		return (
			<div className="flex flex-col gap-4">
				{points.length === 0 ? (
					<p className="max-w-[40rem] text-sm text-ink-muted" role="status">
						{activeStrings().mapNoCoords}
					</p>
				) : (
					<Suspense
						fallback={
							<div className="h-64 animate-pulse rounded-2xl bg-surface-raised" />
						}
					>
						<MapCanvas points={points} />
					</Suspense>
				)}
				<ul className="flex flex-col gap-4">
					{results.map((result) => (
						<li key={result.url}>
							<MapResult
								result={result}
								newTab={newTab}
								query={query}
								onHideSite={onHideSite}
								onPreferSite={onPreferSite}
							/>
						</li>
					))}
				</ul>
			</div>
		);
	}

	return (
		<ul className="flex flex-col gap-8">
			{results.map((result, index) => {
				const Template = specializedTemplate(result, activeCategory);
				return (
					<li
						key={`${result.url}:${engineNames(result).join(",")}`}
						className="zoeken-result"
					>
						{Template ? (
							<Template
								result={result}
								newTab={newTab}
								query={query}
								onHideSite={onHideSite}
								onPreferSite={onPreferSite}
							/>
						) : (
							<ResultItem
								result={result}
								query={query}
								newTab={newTab}
								urlFormatting={urlFormatting}
								cacheUrl={cacheUrl}
								faviconPriority={index < 2}
								onHideSite={onHideSite}
								onPreferSite={onPreferSite}
							/>
						)}
					</li>
				);
			})}
		</ul>
	);
}

function imageRatio(result: SearchResult): string | undefined {
	if (result.kind !== "image" || !result.resolution) return undefined;
	const match = /^(\d+)\s*[x×]\s*(\d+)$/i.exec(result.resolution.trim());
	if (!match) return undefined;
	const width = Number(match[1]);
	const height = Number(match[2]);
	if (!width || !height) return undefined;
	return `${width} / ${height}`;
}

function imageFallback() {
	const node = document.createElement("p");
	node.className =
		"flex h-28 items-center justify-center text-xs text-ink-subtle";
	node.textContent = activeStrings().imageUnavailable;
	return node;
}
