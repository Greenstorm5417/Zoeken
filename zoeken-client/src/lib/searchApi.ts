/** Native search client (`POST /api/v1/search`, MessagePack response). */

import { decode } from "@msgpack/msgpack";
import { ApiError } from "./api";
import type {
	NativeSearchRequest,
	NativeSearchResponse,
} from "./generated/native";

export type SearchParams = {
	q: string;
	pageno?: number;
	language?: string;
	safesearch?: 0 | 1 | 2;
	categories?: string;
	time_range?: string;
	engines?: string;
};

async function getMsgpack<T>(path: string, init?: RequestInit): Promise<T> {
	const res = await fetch(path, {
		...init,
		headers: {
			Accept: "application/msgpack",
			...init?.headers,
		},
	});
	if (!res.ok) {
		throw new ApiError(res.status, await res.text());
	}
	return decode(await res.arrayBuffer()) as T;
}

export function search(params: SearchParams) {
	const body: NativeSearchRequest = {
		q: params.q,
		pageno: params.pageno ?? 1,
		language: params.language ?? null,
		safesearch: params.safesearch ?? null,
		categories: params.categories ?? null,
		time_range: params.time_range ?? null,
		engines: params.engines ?? null,
	};
	return getMsgpack<NativeSearchResponse>("/api/v1/search", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body),
	});
}
