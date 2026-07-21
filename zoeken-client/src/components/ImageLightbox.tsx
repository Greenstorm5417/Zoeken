import { ExternalLink, X } from "lucide-react";
import { useEffect, useId, useRef } from "react";
import type { SearchResult } from "#/lib/api";

function hostnameOf(url: string) {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

const FOCUSABLE =
	'a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])';

/** Full-screen viewer for an image result with source metadata. */
export function ImageLightbox({
	result,
	onClose,
}: {
	result: SearchResult;
	onClose: () => void;
}) {
	const dialogRef = useRef<HTMLDivElement>(null);
	const closeRef = useRef<HTMLButtonElement>(null);
	const previouslyFocused = useRef<HTMLElement | null>(null);
	const titleId = useId();

	useEffect(() => {
		previouslyFocused.current = document.activeElement as HTMLElement | null;
		closeRef.current?.focus();

		const onKey = (event: KeyboardEvent) => {
			if (event.key === "Escape") {
				event.preventDefault();
				onClose();
				return;
			}
			if (event.key !== "Tab" || !dialogRef.current) return;
			const focusable = [
				...dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE),
			].filter((el) => !el.hasAttribute("disabled") && el.tabIndex !== -1);
			if (focusable.length === 0) {
				event.preventDefault();
				return;
			}
			const first = focusable[0];
			const last = focusable[focusable.length - 1];
			const active = document.activeElement as HTMLElement | null;
			if (event.shiftKey && active === first) {
				event.preventDefault();
				last.focus();
			} else if (!event.shiftKey && active === last) {
				event.preventDefault();
				first.focus();
			}
		};

		document.addEventListener("keydown", onKey);
		const prev = document.body.style.overflow;
		document.body.style.overflow = "hidden";
		return () => {
			document.removeEventListener("keydown", onKey);
			document.body.style.overflow = prev;
			previouslyFocused.current?.focus?.();
		};
	}, [onClose]);

	const meta: Array<[string, string]> = [];
	if (result.resolution) meta.push(["Resolution", result.resolution]);
	if (result.img_format) meta.push(["Format", result.img_format]);
	if (result.filesize) meta.push(["Size", result.filesize]);
	if (result.source) meta.push(["Source", result.source]);

	const full = result.img_src || result.thumbnail;

	return (
		<div
			ref={dialogRef}
			className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4 backdrop-blur-sm"
			role="dialog"
			aria-modal="true"
			aria-labelledby={titleId}
			onClick={onClose}
			onKeyDown={(e) => {
				if (e.key === "Enter" || e.key === " ") onClose();
			}}
		>
			<button
				ref={closeRef}
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
					<p id={titleId} className="font-medium text-ink">
						{result.title || "Image"}
					</p>
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
