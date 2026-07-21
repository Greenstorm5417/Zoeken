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
				{initial.description ? (
					<p className="mt-1 text-sm text-ink-subtle">{initial.description}</p>
				) : null}
				{initial.extract ? (
					<p className="mt-3 text-base leading-relaxed text-ink-muted">
						{initial.extract}
					</p>
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
