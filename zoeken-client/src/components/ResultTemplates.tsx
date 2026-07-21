import { Download, ExternalLink, FileText, Magnet } from "lucide-react";
import type { SearchResult } from "#/lib/api";

function hostnameOf(url: string) {
	try {
		return new URL(url).hostname.replace(/^www\./, "");
	} catch {
		return url;
	}
}

function formatEngineLabel(name: string) {
	return name.replace(/[_-]+/g, " ");
}

function engineNames(result: SearchResult): string[] {
	const names =
		result.engines && result.engines.length > 0
			? result.engines
			: result.engine
				? [result.engine]
				: [];
	return [...new Set(names.filter(Boolean))];
}

function EngineLine({ result }: { result: SearchResult }) {
	const engines = engineNames(result);
	if (engines.length === 0) return null;
	return (
		<p className="mt-1.5 text-[0.75rem] text-ink-subtle">
			{engines.map(formatEngineLabel).join(" · ")}
		</p>
	);
}

function ResultTitle({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	return (
		<a
			data-result-link
			href={result.url}
			target={newTab ? "_blank" : undefined}
			rel={newTab ? "noopener noreferrer" : undefined}
			className="group block no-underline"
		>
			<p className="truncate text-[0.75rem] leading-tight text-ink-subtle">
				{hostnameOf(result.url)}
			</p>
			<h2 className="mt-0.5 text-[1.2rem] leading-snug font-medium tracking-tight text-accent group-hover:underline">
				{result.title}
			</h2>
		</a>
	);
}

/** Torrent / downloadable-file result: size, seeders/leechers, magnet button. */
export function TorrentResult({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	const stats: string[] = [];
	if (result.filesize) stats.push(result.filesize);
	if (typeof result.seed === "number") stats.push(`${result.seed} seeders`);
	if (typeof result.leech === "number") stats.push(`${result.leech} leechers`);
	return (
		<article className="max-w-[40rem]">
			<ResultTitle result={result} newTab={newTab} />
			{result.content ? (
				<p className="mt-1 line-clamp-2 text-[0.9rem] text-ink-muted">
					{result.content}
				</p>
			) : null}
			{stats.length > 0 ? (
				<div className="mt-2 flex flex-wrap items-center gap-2 text-[0.8rem]">
					{stats.map((stat) => (
						<span
							key={stat}
							className="rounded-md bg-surface-raised px-2 py-0.5 text-ink-muted ring-1 ring-line/70"
						>
							{stat}
						</span>
					))}
				</div>
			) : null}
			{result.magnetlink ? (
				<a
					href={result.magnetlink}
					className="mt-2.5 inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-[0.8rem] font-medium text-surface no-underline transition-opacity hover:opacity-90"
				>
					<Magnet className="size-3.5" aria-hidden />
					Magnet
				</a>
			) : null}
			<EngineLine result={result} />
		</article>
	);
}

/** Academic paper: authors, journal, DOI, direct PDF link. */
export function PaperResult({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	const meta: string[] = [];
	if (result.authors?.length) meta.push(result.authors.slice(0, 4).join(", "));
	if (result.journal) meta.push(result.journal);
	if (result.publishedDate) meta.push(result.publishedDate.slice(0, 10));
	return (
		<article className="max-w-[40rem]">
			<ResultTitle result={result} newTab={newTab} />
			{meta.length > 0 ? (
				<p className="mt-1 text-[0.8rem] text-ink-subtle">{meta.join(" · ")}</p>
			) : null}
			{result.content ? (
				<p className="mt-1.5 line-clamp-3 text-[0.9rem] text-ink-muted">
					{result.content}
				</p>
			) : null}
			<div className="mt-2 flex flex-wrap items-center gap-3 text-[0.8rem]">
				{result.pdf_url ? (
					<a
						href={result.pdf_url}
						target="_blank"
						rel="noopener noreferrer"
						className="inline-flex items-center gap-1.5 font-medium text-accent hover:underline"
					>
						<FileText className="size-3.5" aria-hidden />
						PDF
					</a>
				) : null}
				{result.doi ? (
					<a
						href={`https://doi.org/${result.doi}`}
						target="_blank"
						rel="noopener noreferrer"
						className="text-ink-subtle hover:text-accent"
					>
						doi:{result.doi}
					</a>
				) : null}
			</div>
			<EngineLine result={result} />
		</article>
	);
}

/** Source-code result: repository, language, highlighted line snippet. */
export function CodeResult({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	const lines = result.codelines ?? [];
	return (
		<article className="max-w-[40rem]">
			<ResultTitle result={result} newTab={newTab} />
			<p className="mt-1 text-[0.8rem] text-ink-subtle">
				{[result.repository, result.code_language, result.filename]
					.filter(Boolean)
					.join(" · ")}
			</p>
			{lines.length > 0 ? (
				<pre className="mt-2 overflow-x-auto rounded-lg border border-line bg-surface-raised p-3 text-[0.78rem] leading-relaxed">
					<code>
						{lines.map(([n, text]) => (
							<div key={n} className="flex gap-3">
								<span className="w-8 shrink-0 select-none text-right text-ink-subtle">
									{n}
								</span>
								<span className="whitespace-pre text-ink">{text}</span>
							</div>
						))}
					</code>
				</pre>
			) : result.content ? (
				<p className="mt-1.5 line-clamp-2 text-[0.9rem] text-ink-muted">
					{result.content}
				</p>
			) : null}
			<EngineLine result={result} />
		</article>
	);
}

/** Key-value / structured record: labeled table. */
export function KeyValueResult({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	const entries = Object.entries(result.kvmap ?? {});
	return (
		<article className="max-w-[40rem]">
			<ResultTitle result={result} newTab={newTab} />
			{entries.length > 0 ? (
				<dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 rounded-lg border border-line bg-surface-raised p-3 text-[0.85rem]">
					{entries.map(([key, value]) => (
						<div key={key} className="contents">
							<dt className="font-medium text-ink-subtle capitalize">{key}</dt>
							<dd className="min-w-0 truncate text-ink">{value}</dd>
						</div>
					))}
				</dl>
			) : result.content ? (
				<p className="mt-1.5 text-[0.9rem] text-ink-muted">{result.content}</p>
			) : null}
			<EngineLine result={result} />
		</article>
	);
}

/** Map / place result: coordinates and links to map providers. */
export function MapResult({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	// OSM result URLs carry mlat/mlon; surface them as a directions link too.
	let lat = "";
	let lon = "";
	try {
		const u = new URL(result.url);
		lat = u.searchParams.get("mlat") ?? "";
		lon = u.searchParams.get("mlon") ?? "";
	} catch {
		// leave blank
	}
	return (
		<article className="max-w-[40rem] rounded-xl border border-line bg-surface-raised p-4">
			<ResultTitle result={result} newTab={newTab} />
			{result.content ? (
				<p className="mt-1.5 line-clamp-2 text-[0.9rem] text-ink-muted">
					{result.content}
				</p>
			) : null}
			<div className="mt-2 flex flex-wrap items-center gap-3 text-[0.8rem]">
				{lat && lon ? (
					<>
						<span className="font-mono text-ink-subtle">
							{Number(lat).toFixed(4)}, {Number(lon).toFixed(4)}
						</span>
						<a
							href={`https://www.openstreetmap.org/?mlat=${lat}&mlon=${lon}#map=15/${lat}/${lon}`}
							target="_blank"
							rel="noopener noreferrer"
							className="inline-flex items-center gap-1 font-medium text-accent hover:underline"
						>
							<ExternalLink className="size-3.5" aria-hidden />
							OpenStreetMap
						</a>
						<a
							href={`https://www.google.com/maps/search/?api=1&query=${lat},${lon}`}
							target="_blank"
							rel="noopener noreferrer"
							className="text-ink-subtle hover:text-accent"
						>
							Google Maps
						</a>
					</>
				) : null}
			</div>
			<EngineLine result={result} />
		</article>
	);
}

/** Pick the specialized template for a result, or `null` for the default. */
export function specializedTemplate(result: SearchResult) {
	switch (result.template) {
		case "file.html":
		case "files.html":
			return TorrentResult;
		case "paper.html":
			return PaperResult;
		case "code.html":
			return CodeResult;
		case "keyvalue.html":
			return KeyValueResult;
		default:
			// Torrents sometimes arrive without a template but with a magnet link.
			if (result.magnetlink) return TorrentResult;
			return null;
	}
}

export { Download };
