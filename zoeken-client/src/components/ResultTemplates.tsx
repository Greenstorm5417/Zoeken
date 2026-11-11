import { ExternalLink, FileText, Magnet } from "lucide-react";
import { type ComponentType, useState } from "react";
import type { SearchResult } from "#/lib/api";
import { resultFavicon } from "#/lib/api";
import { coordsFromResult } from "#/lib/coords";
import { highlightPieces } from "#/lib/highlight";
import { activeStrings } from "#/lib/i18n";
import {
	engineNames,
	formatEngineLabel,
	hostnameOf,
	pathOf,
} from "#/lib/searchDisplay";
import { hostOf } from "#/lib/sourcePrefs";

function uiText() {
	return activeStrings();
}

function Marked({ text, query }: { text: string; query: string }) {
	let cursor = 0;
	return highlightPieces(text, query).map((piece) => {
		const start = cursor;
		cursor += piece.text.length;
		const key = `${start}:${piece.mark ? "m" : "t"}`;
		return piece.mark ? (
			<mark key={key} className="rounded-sm bg-accent-soft text-inherit">
				{piece.text}
			</mark>
		) : (
			<span key={key}>{piece.text}</span>
		);
	});
}

function rememberVisit(url: string) {
	try {
		const raw = sessionStorage.getItem("zoeken.visited") ?? "";
		const next = new Set(raw.split("\n").filter(Boolean));
		next.add(url);
		sessionStorage.setItem("zoeken.visited", [...next].slice(-200).join("\n"));
	} catch {
		// Private mode can reject sessionStorage. The :visited style still applies.
	}
}

function visitedAlready(url: string): boolean {
	try {
		return (sessionStorage.getItem("zoeken.visited") ?? "")
			.split("\n")
			.includes(url);
	} catch {
		return false;
	}
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

function pathCrumbs(url: string): string {
	return pathOf(url).split("/").filter(Boolean).slice(0, 3).join(" › ");
}

function ExpandableText({
	text,
	query = "",
}: {
	text: string;
	query?: string;
}) {
	const [expanded, setExpanded] = useState(false);
	const long = text.length > 180;
	return (
		<>
			<p
				className={`mt-1.5 text-[0.9rem] text-ink-muted ${expanded ? "" : "line-clamp-3"}`}
			>
				<Marked text={text} query={query} />
			</p>
			{long ? (
				<button
					type="button"
					className="mt-1 min-h-11 rounded-lg px-1 text-sm text-accent"
					onClick={() => setExpanded((value) => !value)}
				>
					{expanded ? uiText().showLess : uiText().showMore}
				</button>
			) : null}
		</>
	);
}

function ResultTitle({
	result,
	newTab,
	query = "",
	onHideSite,
	onPreferSite,
}: {
	result: SearchResult;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const domain = hostnameOf(result.url);
	const crumbs = pathCrumbs(result.url);
	const [visited, setVisited] = useState(() => visitedAlready(result.url));
	return (
		<>
			<a
				data-result-link
				data-visited={visited ? "true" : undefined}
				href={result.url}
				target={newTab ? "_blank" : undefined}
				rel={newTab ? "noopener noreferrer" : undefined}
				className="group block no-underline"
				onClick={() => {
					rememberVisit(result.url);
					setVisited(true);
				}}
			>
				<p className="truncate text-[0.75rem] leading-tight text-ink-subtle">
					{domain}
					{crumbs ? ` › ${crumbs}` : ""}
				</p>
				<h2 className="zoeken-result-title mt-0.5 text-[1.2rem] leading-snug font-medium tracking-tight text-accent group-hover:underline">
					<Marked text={result.title || result.url} query={query} />
				</h2>
			</a>
			{domain && (onHideSite || onPreferSite) ? (
				<div className="mt-1 flex flex-wrap gap-2">
					{onHideSite ? (
						<button
							type="button"
							className="min-h-11 rounded-lg px-2 text-xs text-ink-muted hover:text-accent"
							onClick={() => onHideSite(domain)}
						>
							{uiText().hideSite} {domain}
						</button>
					) : null}
					{onPreferSite ? (
						<button
							type="button"
							className="min-h-11 rounded-lg px-2 text-xs text-ink-muted hover:text-accent"
							onClick={() => onPreferSite(domain)}
						>
							{uiText().preferSite} {domain}
						</button>
					) : null}
				</div>
			) : null}
		</>
	);
}

/** Torrent / downloadable-file result: size, seeders/leechers, magnet button. */
export function TorrentResult({
	result,
	newTab,
	query,
	onHideSite,
	onPreferSite,
}: {
	result: Extract<SearchResult, { kind: "file" }>;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const stats: string[] = [];
	const size = result.filesize || result.size;
	if (size) stats.push(size);
	if (result.time) stats.push(result.time);
	if (result.author) stats.push(result.author);
	if (typeof result.seed === "number") stats.push(`${result.seed} seeders`);
	if (typeof result.leech === "number") stats.push(`${result.leech} leechers`);
	const blurb = result.abstract || result.content;
	return (
		<article className="max-w-[40rem]">
			<ResultTitle
				result={result}
				newTab={newTab}
				query={query}
				onHideSite={onHideSite}
				onPreferSite={onPreferSite}
			/>
			{blurb ? <ExpandableText text={blurb} query={query} /> : null}
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

/** Academic paper: authors, citation fields, DOI, direct PDF link. */
export function PaperResult({
	result,
	newTab,
	query,
	onHideSite,
	onPreferSite,
}: {
	result: Extract<SearchResult, { kind: "paper" }>;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const authors = result.authors?.length
		? result.authors.length > 4
			? `${result.authors.slice(0, 4).join(", ")} et al.`
			: result.authors.join(", ")
		: null;
	const citation = [
		result.volume ? `vol. ${result.volume}` : "",
		result.number ? `no. ${result.number}` : "",
		result.pages ? `pp. ${result.pages}` : "",
	]
		.filter(Boolean)
		.join(", ");
	const meta = [
		authors,
		result.type,
		result.journal,
		result.publisher,
		result.editor ? `ed. ${result.editor}` : "",
		citation,
		result.published_date?.slice(0, 10),
	].filter(Boolean) as string[];
	const ids = [
		...(result.issn ?? []).map((id) => `ISSN ${id}`),
		...(result.isbn ?? []).map((id) => `ISBN ${id}`),
	];
	return (
		<article className="max-w-[40rem]">
			<ResultTitle
				result={result}
				newTab={newTab}
				query={query}
				onHideSite={onHideSite}
				onPreferSite={onPreferSite}
			/>
			{meta.length > 0 ? (
				<p className="mt-1 text-[0.8rem] text-ink-subtle">{meta.join(" · ")}</p>
			) : null}
			{ids.length > 0 ? (
				<p className="mt-1 font-mono text-[0.75rem] text-ink-subtle">
					{ids.join(" · ")}
				</p>
			) : null}
			{result.tags && result.tags.length > 0 ? (
				<p className="mt-1 text-[0.75rem] text-ink-subtle">
					{result.tags.slice(0, 6).join(" · ")}
				</p>
			) : null}
			{result.content ? (
				<ExpandableText text={result.content} query={query} />
			) : null}
			{result.comments ? (
				<p className="mt-1 text-[0.8rem] text-ink-subtle">{result.comments}</p>
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
	query,
	onHideSite,
	onPreferSite,
}: {
	result: SearchResult;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	return (
		<article className="max-w-[40rem]">
			<ResultTitle
				result={result}
				newTab={newTab}
				query={query}
				onHideSite={onHideSite}
				onPreferSite={onPreferSite}
			/>
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
	query,
	onHideSite,
	onPreferSite,
}: {
	result: Extract<SearchResult, { kind: "code" }>;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const lines = result.codelines ?? [];
	const hl = new Set(result.hl_lines ?? []);
	return (
		<article className="max-w-[40rem]">
			<ResultTitle
				result={result}
				newTab={newTab}
				query={query}
				onHideSite={onHideSite}
				onPreferSite={onPreferSite}
			/>
			<p className="mt-1 text-[0.8rem] text-ink-subtle">
				{[result.repository, result.code_language, result.filename]
					.filter(Boolean)
					.join(" · ")}
			</p>
			{lines.length > 0 ? (
				<pre className="mt-2 overflow-x-auto rounded-lg border border-line bg-surface-raised p-3 text-[0.78rem] leading-relaxed">
					<code>
						{lines.map(([n, text]) => (
							<div
								key={n}
								className={["flex gap-3", hl.has(n) ? "bg-accent/10" : ""].join(
									" ",
								)}
							>
								<span className="w-8 shrink-0 select-none text-right text-ink-subtle">
									{n}
								</span>
								<span className="whitespace-pre text-ink">{text}</span>
							</div>
						))}
					</code>
				</pre>
			) : result.content ? (
				<ExpandableText text={result.content} query={query} />
			) : null}
			<EngineLine result={result} />
		</article>
	);
}

/** Key-value / structured record: labeled table. */
export function KeyValueResult({
	result,
	newTab,
	query,
	onHideSite,
	onPreferSite,
}: {
	result: Extract<SearchResult, { kind: "key_value" }>;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const entries = result.kvmap ?? [];
	return (
		<article className="max-w-[40rem]">
			<ResultTitle
				result={result}
				newTab={newTab}
				query={query}
				onHideSite={onHideSite}
				onPreferSite={onPreferSite}
			/>
			{result.caption ? (
				<p className="mt-1 text-[0.8rem] text-ink-subtle">{result.caption}</p>
			) : null}
			{entries.length > 0 ? (
				<dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 rounded-lg border border-line bg-surface-raised p-3 text-[0.85rem]">
					{result.key_title || result.value_title ? (
						<div className="contents">
							<dt className="font-semibold text-ink-subtle">
								{result.key_title || "Key"}
							</dt>
							<dd className="font-semibold text-ink">
								{result.value_title || "Value"}
							</dd>
						</div>
					) : null}
					{entries.map(([key, value]) => (
						<div key={`${key}:${value}`} className="contents">
							<dt className="font-medium text-ink-subtle capitalize">{key}</dt>
							<dd className="min-w-0 truncate text-ink">{value}</dd>
						</div>
					))}
				</dl>
			) : result.content ? (
				<ExpandableText text={result.content} query={query} />
			) : null}
			<EngineLine result={result} />
		</article>
	);
}

/** Map / place result: coordinates and links to map providers. */
export function MapResult({
	result,
	newTab,
	query,
	onHideSite,
	onPreferSite,
}: {
	result: SearchResult;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const point = coordsFromResult(result);
	const lat = point ? String(point.lat) : "";
	const lon = point ? String(point.lon) : "";
	return (
		<article className="max-w-[40rem] rounded-xl border border-line bg-surface-raised p-4">
			<ResultTitle
				result={result}
				newTab={newTab}
				query={query}
				onHideSite={onHideSite}
				onPreferSite={onPreferSite}
			/>
			{result.content ? (
				<ExpandableText text={result.content} query={query} />
			) : null}
			<div className="mt-2 flex flex-wrap items-center gap-3 text-[0.8rem]">
				{lat && lon ? (
					<>
						<span className="font-mono text-ink-subtle">
							{point ? `${point.lat.toFixed(4)}, ${point.lon.toFixed(4)}` : ""}
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

type Specialized = ComponentType<{
	result: SearchResult;
	newTab?: boolean;
	query?: string;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}>;

/** Pick the specialized template for a result, or `null` for the default. */
export function specializedTemplate(
	result: SearchResult,
	category?: string,
): Specialized | null {
	switch (result.kind) {
		case "file":
			return TorrentResult as Specialized;
		case "paper":
			return PaperResult as Specialized;
		case "code":
			return CodeResult as Specialized;
		case "key_value":
			return KeyValueResult as Specialized;
		case "main":
			if (category === "shopping" || result.category === "shopping") {
				return ProductResult;
			}
			return null;
		default:
			return null;
	}
}

/** Default (non-specialized) result: favicon, host/path breadcrumb, title, snippet. */
export function ResultItem({
	result,
	query = "",
	newTab = false,
	urlFormatting = "pretty",
	cacheUrl = "",
	faviconPriority = false,
	onHideSite,
	onPreferSite,
}: {
	result: SearchResult;
	query?: string;
	newTab?: boolean;
	urlFormatting?: string;
	cacheUrl?: string;
	/** Bump fetch priority for above-fold SERP favicons (desktop parallel paint). */
	faviconPriority?: boolean;
	onHideSite?: (domain: string) => void;
	onPreferSite?: (domain: string) => void;
}) {
	const host = hostnameOf(result.url);
	const domain = hostOf(result.url);
	const crumbs = pathOf(result.url)
		.split("/")
		.filter(Boolean)
		.slice(0, 3)
		.join(" › ");
	const engines = engineNames(result);
	const displayUrl =
		urlFormatting === "full"
			? result.url
			: urlFormatting === "host"
				? host
				: `${host}${crumbs ? ` › ${crumbs}` : ""}`;
	const [expanded, setExpanded] = useState(false);
	const [visited, setVisited] = useState(() => visitedAlready(result.url));
	const longSnippet = result.content.length > 180;
	const favicon = resultFavicon(result);
	const thumb =
		result.kind === "main" && result.thumbnail ? result.thumbnail : "";
	const published =
		result.kind === "main" && result.published_date
			? result.published_date.slice(0, 10)
			: "";

	return (
		<article className="max-w-[40rem]">
			<a
				data-result-link
				data-visited={visited ? "true" : undefined}
				href={result.url}
				target={newTab ? "_blank" : undefined}
				rel={newTab ? "noopener noreferrer" : undefined}
				className="group block no-underline"
				onClick={() => {
					rememberVisit(result.url);
					setVisited(true);
				}}
			>
				<div className="flex items-center gap-2 sm:gap-2.5">
					{favicon ? (
						<img
							src={favicon}
							alt=""
							width={20}
							height={20}
							className="size-5 shrink-0 rounded-[5px] bg-surface-raised ring-1 ring-line/80"
							decoding="async"
							fetchPriority={faviconPriority ? "high" : undefined}
							onError={(event) => {
								event.currentTarget.hidden = true;
							}}
						/>
					) : null}
					<p className="min-w-0 truncate text-[0.8125rem] leading-tight text-ink-subtle">
						{displayUrl}
					</p>
				</div>
				<h2 className="zoeken-result-title mt-1 text-[1.15rem] leading-snug font-medium tracking-tight text-accent group-hover:underline sm:text-[1.25rem]">
					<Marked text={result.title || result.url} query={query} />
				</h2>
			</a>
			{published ? (
				<p className="mt-1 text-[0.75rem] text-ink-subtle">{published}</p>
			) : null}
			{thumb ? (
				<img
					src={thumb}
					alt=""
					className="mt-2 max-h-28 rounded-lg object-contain"
					decoding="async"
					loading="lazy"
					onError={(event) => {
						event.currentTarget.hidden = true;
					}}
				/>
			) : null}
			{result.content ? (
				<p
					className={`mt-1.5 text-[0.95rem] leading-relaxed text-ink-muted ${expanded ? "" : "line-clamp-3"}`}
				>
					<Marked text={result.content} query={query} />
				</p>
			) : null}
			{longSnippet ? (
				<button
					type="button"
					className="mt-1 min-h-11 text-sm text-accent"
					onClick={() => setExpanded((open) => !open)}
				>
					{expanded ? uiText().showLess : uiText().showMore}
				</button>
			) : null}
			{engines.length > 0 || cacheUrl || domain ? (
				<details className="mt-1 text-[0.75rem] text-ink-subtle">
					<summary className="cursor-pointer min-h-11 list-none py-1 marker:content-none">
						{engines.length
							? engines.map(formatEngineLabel).join(" · ")
							: "Source"}
					</summary>
					<div className="flex flex-wrap gap-2 pb-1">
						{domain && onHideSite ? (
							<button
								type="button"
								className="min-h-11 rounded-lg px-2 text-ink-muted hover:text-accent"
								onClick={() => onHideSite(domain)}
							>
								{uiText().hideSite} {domain}
							</button>
						) : null}
						{domain && onPreferSite ? (
							<button
								type="button"
								className="min-h-11 rounded-lg px-2 text-ink-muted hover:text-accent"
								onClick={() => onPreferSite(domain)}
							>
								{uiText().preferSite} {domain}
							</button>
						) : null}
						{cacheUrl ? (
							<a
								href={cacheUrl + encodeURIComponent(result.url)}
								target="_blank"
								rel="noopener noreferrer"
								className="inline-flex min-h-11 items-center text-ink-subtle underline decoration-line underline-offset-2 hover:text-accent"
							>
								archive
							</a>
						) : null}
					</div>
				</details>
			) : null}
		</article>
	);
}
