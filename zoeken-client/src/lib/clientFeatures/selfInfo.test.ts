import { describe, expect, it } from "vitest";
import { computeSelfInfoAnswer } from "./selfInfo";

describe("computeSelfInfoAnswer", () => {
	it("answers ip queries with the client IP", () => {
		const answer = computeSelfInfoAnswer(
			"what's my ip",
			1,
			"203.0.113.9",
			"ua",
		);
		expect(answer?.answer).toBe("Your IP is: 203.0.113.9");
		expect(answer?.interactive).toEqual({
			type: "self_info",
			kind: "ip",
			value: "203.0.113.9",
		});
	});

	it("is tolerant of punctuation and casing", () => {
		const answer = computeSelfInfoAnswer(
			"What Is My IP?",
			1,
			"203.0.113.9",
			"ua",
		);
		expect(answer?.engine).toBe("self_info");
	});

	it("reports unavailable when there's no client IP", () => {
		const answer = computeSelfInfoAnswer("my ip", 1, null, "ua");
		expect(answer?.answer).toBe("Your IP is unavailable");
	});

	it("answers user-agent queries with the browser UA", () => {
		const answer = computeSelfInfoAnswer(
			"my user agent",
			1,
			null,
			"Mozilla/5.0",
		);
		expect(answer?.answer).toBe("Your user-agent is: Mozilla/5.0");
		expect(answer?.interactive).toEqual({
			type: "self_info",
			kind: "user_agent",
			value: "Mozilla/5.0",
		});
	});

	it("does not match ip inside an unrelated sentence", () => {
		expect(
			computeSelfInfoAnswer("skip the tutorial", 1, "1.2.3.4", "ua"),
		).toBeNull();
	});

	it("returns null past the first page", () => {
		expect(computeSelfInfoAnswer("my ip", 2, "1.2.3.4", "ua")).toBeNull();
	});
});
