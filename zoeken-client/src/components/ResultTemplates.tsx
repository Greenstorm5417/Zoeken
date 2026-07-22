import { Download, ExternalLink, FileText, Magnet } from "lucide-react";
import type { SearchResult } from "#/lib/api";
import {
	engineNames,
	formatEngineLabel,
	hostnameOf,
	pathOf,
} from "#/lib/searchDisplay";

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
	const authors = result.authors?.length
		? result.authors.length > 4
			? `${result.authors.slice(0, 4).join(", ")} et al.`
			: result.authors.join(", ")
		: null;
	const meta = [
		authors,
		result.journal,
		result.publisher,
		result.publishedDate?.slice(0, 10),
	].filter(Boolean) as string[];
	return (
		<article className="max-w-[40rem]">
			<ResultTitle result={result} newTab={newTab} />
			{meta.length > 0 ? (
				<p className="mt-1 text-[0.8rem] text-ink-subtle">{meta.join(" · ")}</p>
			) : null}
			{result.tags && result.tags.length > 0 ? (
				<p className="mt-1 text-[0.75rem] text-ink-subtle">
					{result.tags.slice(0, 6).join(" · ")}
				</p>
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
				{result.html_url ? (
					<a
						href={result.html_url}
						target="_blank"
						rel="noopener noreferrer"
						className="text-ink-subtle hover:text-accent"
					>
						HTML
					</a>
				) : null}
				{result.doi ? (
					<a
						href={`https://doi.org/${result.doi}`}
						target="_blank"
						rel="noopener noreferrer"
						className="font-mono text-ink-subtle hover:text-accent"
					>
						doi:{result.doi}
					</a>
				) : null}
			</div>
			<EngineLine result={result} />
		</article>
	);
}

/** Shopping / product listing: title, price-or-snippet in content, link. */
export function ProductResult({
	result,
	newTab,
}: {
	result: SearchResult;
	newTab?: boolean;
}) {
	return (
		<article className="max-w-[40rem]">
			<ResultTitle result={result} newTab={newTab} />
			{result.content ? (
				<p className="mt-1.5 text-[1.05rem] font-medium tracking-tight text-ink">
					{result.content}
				</p>
			) : null}
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
	let lat = "";
	let lon = "";
	try {
		const u = new URL(result.url);
		lat = u.searchParams.get("mlat") ?? u.searchParams.get("lat") ?? "";
		lon = u.searchParams.get("mlon") ?? u.searchParams.get("lon") ?? "";
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
export function specializedTemplate(result: SearchResult, category?: string) {
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
		case "products.html":
		case "product.html":
			return ProductResult;
		default:
			// Torrents sometimes arrive without a template but with a magnet link.
			if (result.magnetlink) return TorrentResult;
			// ebay/geizhals put price in content — no dedicated product fields.
			if (category === "shopping") return ProductResult;
			return null;
	}
}

export { Download };

/** Default (non-specialized) result: favicon, host/path breadcrumb, title, snippet. */
export function ResultItem({
	result,
	newTab = false,
	urlFormatting = "pretty",
	cacheUrl = "",
}: {
	result: SearchResult;
	newTab?: boolean;
	urlFormatting?: string;
	cacheUrl?: string;
}) {
	const host = hostnameOf(result.url);
	const crumbs = pathOf(result.url)
		.split("/")
		.filter(Boolean)
		.slice(0, 3)
		.join(" > ");
	const engines = engineNames(result);
	const displayUrl =
		urlFormatting === "full"
			? result.url
			: urlFormatting === "host"
				? host
				: `${host}${crumbs ? ` > ${crumbs}` : ""}`;

	return (
		<article className="max-w-[40rem]">
			<a
				data-result-link
				href={result.url}
				target={newTab ? "_blank" : undefined}
				rel={newTab ? "noopener noreferrer" : undefined}
				className="group block no-underline"
			>
				<div className="flex items-center gap-2.5">
					{result.favicon ? (
						<img
							src={result.favicon}
							alt=""
							width={20}
							height={20}
							className="size-5 rounded-[5px] bg-surface-raised ring-1 ring-line/80"
							loading="lazy"
							onError={(event) => {
								event.currentTarget.hidden = true;
							}}
						/>
					) : null}
					<div className="min-w-0">
						<p className="truncate text-[0.875rem] leading-tight text-ink">
							{host}
						</p>
						<p className="truncate text-[0.75rem] leading-tight text-ink-subtle">
							{displayUrl}
						</p>
					</div>
				</div>
				<h2 className="mt-1.5 text-[1.25rem] leading-snug font-medium tracking-tight text-accent transition-colors group-hover:underline">
					{result.title}
				</h2>
			</a>
			{result.content ? (
				<p className="mt-1.5 line-clamp-2 text-[0.9rem] leading-relaxed text-ink-muted">
					{result.content}
				</p>
			) : null}
			{engines.length > 0 || cacheUrl ? (
				<p className="mt-1.5 text-[0.75rem] text-ink-subtle">
					{engines.map(formatEngineLabel).join(" · ")}
					{cacheUrl ? (
						<>
							{engines.length > 0 ? <span aria-hidden> · </span> : null}
							<a
								href={cacheUrl + encodeURIComponent(result.url)}
								target="_blank"
								rel="noopener noreferrer"
								className="text-ink-subtle underline decoration-line underline-offset-2 hover:text-accent"
							>
								archive
							</a>
						</>
					) : null}
				</p>
			) : null}
		</article>
	);
}
