import { describe, expect, it } from "vitest";
import { nextPageNumber, showNumberedNext } from "./pagination";

describe("pagination", () => {
	it("does not treat a non-empty page as proof of another page when exhausted", () => {
		const page = {
			continuation: "exhausted",
			results: [{ url: "https://a.test/" }],
		};
		expect(showNumberedNext(page)).toBe(false);
		expect(nextPageNumber([page], 1)).toBeUndefined();
	});

	it("keeps a later page after one duplicate page when continuation is more", () => {
		const first = {
			continuation: "more",
			results: [{ url: "https://a.test/" }],
		};
		const repeat = {
			continuation: "more",
			results: [{ url: "https://a.test/" }],
		};
		const third = {
			continuation: "more",
			results: [{ url: "https://b.test/" }],
		};
		expect(nextPageNumber([first], 1)).toBe(2);
		expect(nextPageNumber([first, repeat], 1)).toBe(3);
		expect(showNumberedNext(repeat, [first])).toBe(true);
		expect(nextPageNumber([first, repeat, third], 1)).toBe(4);
	});

	it("retries one empty unknown page and then recovers", () => {
		const failed = {
			continuation: "unknown",
			results: [] as Array<{ url: string }>,
		};
		const recovered = {
			continuation: "more",
			results: [{ url: "https://b.test/" }],
		};
		expect(nextPageNumber([failed], 1)).toBe(2);
		expect(showNumberedNext(failed)).toBe(true);
		expect(nextPageNumber([failed, failed], 1)).toBeUndefined();
		expect(nextPageNumber([failed, recovered], 1)).toBe(3);
	});
});
