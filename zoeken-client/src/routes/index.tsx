import { createFileRoute, Link } from "@tanstack/react-router";
import { SearchForm } from "#/components/SearchForm";
import { SiteNav } from "#/components/SiteNav";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
	return (
		<main className="mx-auto flex min-h-dvh w-full max-w-3xl flex-col items-center justify-center px-6 pt-16 pb-[16vh]">
			<SiteNav />
			<div className="animate-rise flex w-full flex-col items-center gap-10">
				<Link
					to="/"
					className="flex items-center justify-center gap-4 no-underline"
				>
					<img
						src="/zoeken-logo.svg"
						alt=""
						width={56}
						height={56}
						className="size-12 sm:size-14"
					/>
					<h1 className="text-5xl font-bold tracking-tight text-ink sm:text-6xl">
						Zoeken
					</h1>
				</Link>

				<SearchForm autoFocus />
			</div>
		</main>
	);
}
