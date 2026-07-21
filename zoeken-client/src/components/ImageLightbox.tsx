import { ExternalLink, X } from "lucide-react";
import { useEffect } from "react";
import type { SearchResult } from "#/lib/api";

function hostnameOf(url: string) {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

/** Full-screen viewer for an image result with source metadata. */
export function ImageLightbox({
	result,
	onClose,
}: {
	result: SearchResult;
	onClose: () => void;
}) {
	useEffect(() => {
		const onKey = (event: KeyboardEvent) => {
			if (event.key === "Escape") onClose();
		};
		document.addEventListener("keydown", onKey);
		// Lock background scroll while the overlay is open.
		const prev = document.body.style.overflow;
		document.body.style.overflow = "hidden";
		return () => {
			document.removeEventListener("keydown", onKey);
			document.body.style.overflow = prev;
		};
	}, [onClose]);

	const meta: Array<[string, string]> = [];
	if (result.resolution) meta.push(["Resolution", result.resolution]);
	if (result.img_format) meta.push(["Format", result.img_format]);
	if (result.filesize) meta.push(["Size", result.filesize]);
	if (result.source) meta.push(["Source", result.source]);

	const full = result.img_src ?? result.thumbnail;

	return (
		<div
			className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4 backdrop-blur-sm"
			role="dialog"
			aria-modal="true"
			aria-label={result.title || "Image preview"}
			onClick={onClose}
			onKeyDown={(e) => {
				if (e.key === "Enter" || e.key === " ") onClose();
			}}
		>
			<button
				type="button"
				onClick={onClose}
				aria-label="Close"
				className="absolute top-4 right-4 rounded-full bg-white/10 p-2 text-white transition-colors hover:bg-white/20"
			>
				<X className="size-5" aria-hidden />
			</button>
			{/* biome-ignore lint/a11y/noStaticElementInteractions: event boundary only, stops clicks/keys inside the dialog panel from bubbling to the backdrop's onClose */}
			<div
				className="flex max-h-full w-full max-w-4xl flex-col items-center gap-4"
				onClick={(e) => e.stopPropagation()}
				onKeyDown={(e) => e.stopPropagation()}
			>
				{full ? (
					<img
						src={full}
						alt={result.title}
						className="max-h-[70vh] max-w-full rounded-lg object-contain"
					/>
				) : null}
				<div className="w-full max-w-2xl rounded-xl bg-surface-raised p-4">
					<p className="font-medium text-ink">{result.title || "Image"}</p>
					{meta.length > 0 ? (
						<dl className="mt-2 flex flex-wrap gap-x-6 gap-y-1 text-sm">
							{meta.map(([label, value]) => (
								<div key={label} className="flex gap-1.5">
									<dt className="text-ink-subtle">{label}:</dt>
									<dd className="text-ink">{value}</dd>
								</div>
							))}
						</dl>
					) : null}
					<div className="mt-3 flex flex-wrap gap-3 text-sm">
						<a
							href={result.url}
							target="_blank"
							rel="noopener noreferrer"
							className="inline-flex items-center gap-1.5 font-medium text-accent hover:underline"
						>
							<ExternalLink className="size-3.5" aria-hidden />
							{hostnameOf(result.url)}
						</a>
						{result.img_src ? (
							<a
								href={result.img_src}
								target="_blank"
								rel="noopener noreferrer"
								className="text-ink-subtle hover:text-accent"
							>
								Open full image
							</a>
						) : null}
					</div>
				</div>
			</div>
		</div>
	);
}
