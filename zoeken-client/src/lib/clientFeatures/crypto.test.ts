import { describe, expect, it } from "vitest";
import { computeCryptoAnswer } from "./crypto";

describe("computeCryptoAnswer", () => {
	it("detects hash prefix and suffix phrasing", () => {
		expect(computeCryptoAnswer("sha256 abc")?.interactive).toEqual({
			type: "crypto",
			mode: "hash",
			algorithm: "sha256",
			input: "abc",
		});
		expect(computeCryptoAnswer("hello sha256")?.interactive).toEqual({
			type: "crypto",
			mode: "hash",
			algorithm: "sha256",
			input: "hello",
		});
		expect(computeCryptoAnswer("hash the fox with md5")?.interactive).toEqual({
			type: "crypto",
			mode: "hash",
			algorithm: "md5",
			input: "the fox",
		});
	});

	it("detects encode/decode phrases", () => {
		expect(computeCryptoAnswer("base64 hello")?.interactive).toEqual({
			type: "crypto",
			mode: "encode",
			algorithm: "base64",
			input: "hello",
		});
		expect(computeCryptoAnswer("base 64 encode hello world")?.interactive).toEqual({
			type: "crypto",
			mode: "encode",
			algorithm: "base64",
			input: "hello world",
		});
		expect(computeCryptoAnswer("what is hello in base64")?.interactive).toEqual({
			type: "crypto",
			mode: "encode",
			algorithm: "base64",
			input: "hello",
		});
		expect(computeCryptoAnswer("decode base64 aGVsbG8=")?.interactive).toEqual({
			type: "crypto",
			mode: "decode",
			algorithm: "base64",
			input: "aGVsbG8=",
		});
		expect(computeCryptoAnswer("url encode hello world")?.interactive).toEqual({
			type: "crypto",
			mode: "encode",
			algorithm: "url",
			input: "hello world",
		});
		expect(computeCryptoAnswer("hex encode abc")?.interactive).toEqual({
			type: "crypto",
			mode: "encode",
			algorithm: "hex",
			input: "abc",
		});
	});

	it("ignores unrelated queries", () => {
		expect(computeCryptoAnswer("rust programming")).toBeNull();
		expect(computeCryptoAnswer("sha256")).toBeNull();
		expect(computeCryptoAnswer("")).toBeNull();
		expect(computeCryptoAnswer("random sha256")).toBeNull();
	});

	it("sets an intent-only answer without computing the digest", () => {
		const answer = computeCryptoAnswer("sha256 abc");
		expect(answer?.answer).toContain("sha256");
		expect(answer?.answer).not.toContain("ba7816");
		expect(answer?.engine).toBe("hash_plugin");
	});
});
