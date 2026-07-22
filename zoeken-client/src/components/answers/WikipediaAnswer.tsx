import { Link } from "@tanstack/react-router";
import { Library } from "lucide-react";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";

export function WikipediaAnswer({
	answer,
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "wikipedia" }>;
}) {
	const href = initial.url || answer.url;
	const img = initial.img_src || undefined;
	const qid = initial.wikidata_id?.trim() || "";
	const attributes = (initial.attributes ?? []).filter(
		(attr) => attr.label && (attr.value || attr.image?.src),
	);
	const topics = (initial.related_topics ?? []).filter(Boolean);

	return (
		<section className="mb-6 max-w-[40rem] overflow-hidden rounded-2xl border border-line bg-surface-raised">
			{img ? (
				<img src={img} alt="" className="max-h-48 w-full object-cover" />
			) : null}
			<div className="px-5 py-4">
				<p className="mb-2 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
					<Library className="size-4 text-accent" aria-hidden />
					Wikipedia
				</p>
				<h2 className="text-[1.35rem] leading-snug tracking-tight text-ink">
					{href ? (
						<a
							href={href}
							target="_blank"
							rel="noopener noreferrer"
							className="text-ink no-underline hover:text-accent hover:underline"
						>
							{initial.title}
						</a>
					) : (
						initial.title
					)}
				</h2>
				{qid ? (
					<p className="mt-1 font-mono text-xs text-ink-subtle">
						<a
							href={`https://www.wikidata.org/wiki/${qid}`}
							target="_blank"
							rel="noopener noreferrer"
							className="text-ink-subtle no-underline hover:text-accent hover:underline"
						>
							{qid}
						</a>
					</p>
				) : null}
				{initial.description ? (
					<p className="mt-1 text-sm text-ink-subtle">{initial.description}</p>
				) : null}
				{initial.extract ? (
					<p className="mt-3 text-base leading-relaxed text-ink-muted">
						{initial.extract}
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
				{href ? (
					<a
						href={href}
						target="_blank"
						rel="noopener noreferrer"
						className="mt-3 inline-block text-sm text-accent hover:underline"
					>
						Read on Wikipedia
					</a>
				) : null}
			</div>
		</section>
	);
}
