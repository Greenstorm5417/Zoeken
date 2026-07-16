import { TanStackDevtools } from "@tanstack/react-devtools";
import { type QueryClient, useQuery } from "@tanstack/react-query";
import { ReactQueryDevtoolsPanel } from "@tanstack/react-query-devtools";
import { createRootRouteWithContext, Outlet } from "@tanstack/react-router";
import { TanStackRouterDevtoolsPanel } from "@tanstack/react-router-devtools";
import { createContext, useContext } from "react";
import { type Config, config } from "#/lib/api";
import "../styles.css";

const ConfigContext = createContext<Config | undefined>(undefined);

export function useConfig() {
	return useContext(ConfigContext);
}

export const Route = createRootRouteWithContext<{
	queryClient: QueryClient;
}>()({
	component: RootComponent,
});

function RootComponent() {
	const configQuery = useQuery({ queryKey: ["config"], queryFn: config });
	return (
		<ConfigContext.Provider value={configQuery.data}>
			<Outlet />
			{import.meta.env.DEV ? (
				<TanStackDevtools
					config={{ position: "bottom-right" }}
					plugins={[
						{
							name: "TanStack Router",
							render: <TanStackRouterDevtoolsPanel />,
						},
						{
							name: "TanStack Query",
							render: <ReactQueryDevtoolsPanel />,
						},
					]}
				/>
			) : null}
		</ConfigContext.Provider>
	);
}
