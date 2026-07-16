import { createFileRoute, Link } from "@tanstack/react-router";
import { SearchForm } from "#/components/SearchForm";
import { SiteNav } from "#/components/SiteNav";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
	return (
		<main className="mx-auto flex min-h-dvh w-full max-w-3xl flex-col items-center justify-center px-6 py-16">
			<SiteNav />
			<div className="animate-rise flex w-full flex-col items-center gap-10">
				<Link to="/" className="flex flex-col items-center gap-3 no-underline">
					<img src="/zoeken-logo.svg" alt="" width={72} height={72} />
					<h1 className="text-5xl font-bold tracking-tight text-ink sm:text-6xl">
						Zoeken
					</h1>
				</Link>

				<SearchForm autoFocus />
			</div>
		</main>
	);
}
