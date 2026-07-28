import { Link } from "@tanstack/react-router";
import type { Infobox } from "#/lib/api";
import { formatEngineLabel, wikidataId } from "#/lib/searchDisplay";

export function InfoboxCard({ box }: { box: Infobox }) {
	const title = box.infobox || "Info";
	const qid = wikidataId(box.id);
	const source = box.engine ? formatEngineLabel(box.engine) : null;
	const primaryUrl = box.urls?.[0]?.url ?? box.id ?? undefined;
	const attributes = (box.attributes ?? []).filter(
		(attr) => attr.label && (attr.value || attr.image?.src),
	);
	const topics = (box.related_topics ?? []).filter(Boolean);

	return (
		<article className="mb-4 overflow-hidden rounded-2xl border border-line bg-surface-raised">
			{box.img_src ? (
				<img
					src={box.img_src}
					alt=""
					className="max-h-44 w-full object-cover"
					decoding="async"
					loading="lazy"
				/>
			) : null}
			<div className="p-4">
				{source ? (
					<p className="mb-1.5 text-[0.7rem] font-medium tracking-wide text-ink-subtle uppercase">
						{source}
					</p>
				) : null}
				<h3 className="text-base font-medium text-ink">
					{primaryUrl ? (
						<a
							href={primaryUrl}
							target="_blank"
							rel="noopener noreferrer"
							className="text-ink no-underline hover:text-accent hover:underline"
						>
							{title}
						</a>
					) : (
						title
					)}
				</h3>
				{qid ? (
					<p className="mt-1 font-mono text-xs text-ink-subtle">{qid}</p>
				) : null}
				{box.content ? (
					<p className="mt-2 text-sm leading-relaxed text-ink-muted">
						{box.content}
					</p>
				) : null}
				{attributes.length > 0 ? (
					<dl className="mt-3 space-y-2 border-t border-line pt-3">
						{attributes.map((attr) => (
							<div key={`${attr.label}:${attr.value ?? ""}`}>
								<dt className="text-[0.7rem] font-medium tracking-wide text-ink-subtle uppercase">
									{attr.label}
								</dt>
								{attr.image?.src ? (
									<dd className="mt-1">
										<img
											src={attr.image.src}
											alt={attr.image.alt || attr.label}
											className="max-h-24 rounded-lg object-contain"
											decoding="async"
											loading="lazy"
										/>
									</dd>
								) : null}
								{attr.value ? (
									<dd className="mt-0.5 text-sm text-ink">{attr.value}</dd>
								) : null}
							</div>
						))}
					</dl>
				) : null}
				{topics.length > 0 ? (
					<div className="mt-3 border-t border-line pt-3">
						<p className="mb-1.5 text-[0.7rem] font-medium tracking-wide text-ink-subtle uppercase">
							Related
						</p>
						<ul className="flex flex-wrap gap-1.5">
							{topics.map((topic) => (
								<li key={topic}>
									<Link
										to="/search"
										search={{ q: topic }}
										className="inline-block rounded-lg border border-line px-2 py-0.5 text-xs text-ink no-underline hover:border-accent hover:text-accent"
									>
										{topic}
									</Link>
								</li>
							))}
						</ul>
					</div>
				) : null}
				{box.urls && box.urls.length > 0 ? (
					<ul className="mt-3 flex flex-col gap-1">
						{box.urls.map((link) => (
							<li key={link.url}>
								<a
									href={link.url}
									target="_blank"
									rel="noopener noreferrer"
									className="text-sm text-accent hover:underline"
								>
									{link.title || "Source"}
								</a>
							</li>
						))}
					</ul>
				) : null}
			</div>
		</article>
	);
}
