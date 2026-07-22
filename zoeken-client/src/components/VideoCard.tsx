import { useState } from "react";
import type { SearchResult } from "#/lib/api";
import { resultIframeSrc, resultThumbnail } from "#/lib/api";
import { engineNames, formatEngineLabel } from "#/lib/searchDisplay";

/** Sandboxed click-to-play video card when `iframe_src` is present. */
export function VideoCard({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	const [playing, setPlaying] = useState(false);
	const thumb = resultThumbnail(result);
	const embed = resultIframeSrc(result).trim();

	return (
		<article className="overflow-hidden rounded-xl border border-line bg-surface-raised">
			<div className="aspect-video bg-ink/5">
				{playing && embed ? (
					<iframe
						src={embed}
						title={result.title}
						className="size-full border-0"
						allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
						sandbox="allow-scripts allow-same-origin allow-presentation allow-popups"
						allowFullScreen
					/>
				) : (
					<button
						type="button"
						onClick={() => {
							if (embed) setPlaying(true);
							else window.open(result.url, newTab ? "_blank" : "_self");
						}}
						className="group relative block size-full"
						aria-label={embed ? `Play ${result.title}` : result.title}
					>
						{thumb ? (
							<img
								src={thumb}
								alt=""
								className="size-full object-cover transition-transform duration-150 group-hover:scale-[1.01]"
								loading="lazy"
							/>
						) : (
							<div className="flex size-full items-center justify-center text-sm text-ink-subtle">
								Video
							</div>
						)}
						{embed ? (
							<span className="absolute inset-0 flex items-center justify-center bg-black/25 text-white opacity-90 transition-opacity group-hover:opacity-100">
								<span className="rounded-full bg-black/60 px-3 py-1.5 text-sm font-medium">
									Play
								</span>
							</span>
						) : null}
					</button>
				)}
			</div>
			<div className="p-3">
				{embed && playing ? (
					<p className="line-clamp-2 text-sm font-medium text-ink">
						{result.title}
					</p>
				) : (
					<a
						data-result-link
						href={result.url}
						target={newTab ? "_blank" : undefined}
						rel={newTab ? "noopener noreferrer" : undefined}
						className="line-clamp-2 text-sm font-medium text-ink no-underline hover:text-accent"
					>
						{result.title}
					</a>
				)}
				{result.content ? (
					<p className="mt-1 line-clamp-2 text-xs text-ink-muted">
						{result.content}
					</p>
				) : null}
				{result.kind === "main" && (result.author || result.length) ? (
					<p className="mt-1 truncate text-[0.7rem] text-ink-subtle">
						{[result.author, result.length].filter(Boolean).join(" · ")}
					</p>
				) : null}
				{engineNames(result).length ? (
					<p className="mt-1.5 truncate text-[0.65rem] text-ink-subtle">
						{engineNames(result).map(formatEngineLabel).join(" · ")}
					</p>
				) : null}
			</div>
		</article>
	);
}
