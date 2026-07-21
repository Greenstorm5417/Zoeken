import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig(async ({ mode }) => {
	const plugins = [
		tailwindcss(),
		tanstackRouter({ target: "react", autoCodeSplitting: true }),
		viteReact(),
	];

	// Devtools Vite plugin is development-only (keeps production bundles lean).
	if (mode === "development") {
		const { devtools } = await import("@tanstack/devtools-vite");
		plugins.unshift(devtools());
	}

	return {
		resolve: {
			alias: {
				"#": path.resolve(__dirname, "./src"),
				"@": path.resolve(__dirname, "./src"),
			},
		},
		plugins,
		server: {
			port: 3000,
			proxy: {
				// SearXNG-compatible API surface on zoeken-server (default :8888).
				"/search": "http://127.0.0.1:8888",
				"/autocompleter": "http://127.0.0.1:8888",
				"/config": "http://127.0.0.1:8888",
				"/preferences": "http://127.0.0.1:8888",
				"/clear_cookies": "http://127.0.0.1:8888",
				"/stats": "http://127.0.0.1:8888",
				"/metrics": "http://127.0.0.1:8888",
				"/opensearch.xml": "http://127.0.0.1:8888",
				"/engine_descriptions.json": "http://127.0.0.1:8888",
				"/image_proxy": "http://127.0.0.1:8888",
				"/favicon_proxy": "http://127.0.0.1:8888",
				"/favicon.ico": "http://127.0.0.1:8888",
				"/manifest.json": "http://127.0.0.1:8888",
				"/robots.txt": "http://127.0.0.1:8888",
				"/sitemap.xml": "http://127.0.0.1:8888",
				"/info": "http://127.0.0.1:8888",
				"/logo": "http://127.0.0.1:8888",
				"/about": "http://127.0.0.1:8888",
			},
		},
		build: {
			outDir: path.resolve(__dirname, "../zoeken/zoeken-server/assets"),
			emptyOutDir: true,
		},
	};
});
