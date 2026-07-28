import { coordsFromResult, MapCanvas } from "#/components/MapCanvas";
import {
	MapResult,
	ResultItem,
	specializedTemplate,
} from "#/components/ResultTemplates";
import { VideoCard } from "#/components/VideoCard";
import type { SearchResult } from "#/lib/api";
import { resultImgSrc, resultThumbnail } from "#/lib/api";
import { engineNames } from "#/lib/searchDisplay";

type Props = {
	results: SearchResult[];
	activeCategory: string;
	videoMode: boolean;
	imageMode: boolean;
	mapMode: boolean;
	newTab?: boolean;
	urlFormatting?: string;
	cacheUrl?: string;
	onOpenImage: (result: SearchResult) => void;
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
	onOpenImage,
	empty,
	emptyTitle,
	emptyHint,
}: Props) {
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
					<li key={result.url}>
						<VideoCard result={result} newTab={newTab} />
					</li>
				))}
			</ul>
		);
	}

	if (imageMode) {
		return (
			<ul className="grid max-w-5xl grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
				{results
					.filter((r) => resultThumbnail(r) || resultImgSrc(r))
					.map((result) => (
						<li key={result.url}>
							<button
								type="button"
								onClick={() => onOpenImage(result)}
								className="group block w-full overflow-hidden rounded-xl text-left"
							>
								<img
									src={resultThumbnail(result) || resultImgSrc(result)}
									alt={result.title || ""}
									className="aspect-square w-full bg-surface-raised object-cover transition-transform duration-150 group-hover:scale-[1.01]"
									decoding="async"
									loading="lazy"
								/>
								<p className="mt-1.5 truncate text-xs text-ink-muted group-hover:text-accent">
									{result.title || "Image"}
								</p>
								{result.kind === "image" &&
								result.resolution &&
								result.resolution !== "unknown" ? (
									<p className="truncate text-[0.65rem] text-ink-subtle">
										{result.resolution}
										{result.img_format ? ` · ${result.img_format}` : ""}
									</p>
								) : null}
							</button>
						</li>
					))}
			</ul>
		);
	}

	if (mapMode) {
		return (
			<div className="flex flex-col gap-4">
				<MapCanvas
					points={results
						.map(coordsFromResult)
						.filter((p): p is NonNullable<typeof p> => p != null)}
				/>
				<ul className="flex flex-col gap-4">
					{results.map((result) => (
						<li key={result.url}>
							<MapResult result={result} newTab={newTab} />
						</li>
					))}
				</ul>
			</div>
		);
	}

	return (
		<ul className="flex flex-col gap-8">
			{results.map((result) => {
				const Template = specializedTemplate(result, activeCategory);
				return (
					<li key={`${result.url}:${engineNames(result).join(",")}`}>
						{Template ? (
							<Template result={result} newTab={newTab} />
						) : (
							<ResultItem
								result={result}
								newTab={newTab}
								urlFormatting={urlFormatting}
								cacheUrl={cacheUrl}
							/>
						)}
					</li>
				);
			})}
		</ul>
	);
}
