import type { ComponentType, ReactNode, SVGProps } from "react";

type Props = {
	title: string;
	icon?: ComponentType<SVGProps<SVGSVGElement> & { className?: string }>;
	children: ReactNode;
	/** Wider tool layouts (converter, crypto). Default matches SERP column. */
	wide?: boolean;
	/** Extra classes on the section (e.g. calculator landscape). */
	className?: string;
	footer?: ReactNode;
};

/** Shared chrome for instant-answer cards — matches SERP surface/line tokens. */
export function AnswerShell({
	title,
	icon: Icon,
	children,
	wide = true,
	className = "",
	footer,
}: Props) {
	return (
		<section
			className={[
				"zoeken-answer mb-6 rounded-2xl border border-line bg-surface-raised",
				wide ? "max-w-[40rem]" : "max-w-[36rem]",
				className,
			]
				.filter(Boolean)
				.join(" ")}
		>
			<div className="px-4 py-4 sm:px-5">
				<p className="mb-3 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
					{Icon ? <Icon className="size-4 text-accent" aria-hidden /> : null}
					{title}
				</p>
				{children}
				{footer ? <div className="mt-3">{footer}</div> : null}
			</div>
		</section>
	);
}
