/** Local "what's my ip / user-agent" answerer. IP comes from `/config`; UA from the browser. */
import type { SearchAnswer } from "../api";

// Whole-query phrases only — avoids matching "ip" inside arbitrary sentences.
const IP_QUERIES = new Set([
	"ip",
	"my ip",
	"ip address",
	"my ip address",
	"whats my ip",
	"what's my ip",
	"what is my ip",
	"whats my ip address",
	"what's my ip address",
	"what is my ip address",
	"show my ip",
	"show my ip address",
]);

const UA_QUERIES = new Set([
	"user-agent",
	"user agent",
	"my user-agent",
	"my user agent",
	"whats my user-agent",
	"whats my user agent",
	"what's my user-agent",
	"what's my user agent",
	"what is my user-agent",
	"what is my user agent",
	"show my user-agent",
	"show my user agent",
]);

function normalize(query: string): string {
	return query.toLowerCase().replaceAll("?", "").replace(/\s+/g, " ").trim();
}

export function computeSelfInfoAnswer(
	query: string,
	pageno: number,
	clientIp: string | null,
	userAgent: string,
): SearchAnswer | null {
	if (pageno > 1) return null;
	const q = normalize(query);

	if (IP_QUERIES.has(q)) {
		const ip = clientIp ?? "";
		return {
			answer: ip ? `Your IP is: ${ip}` : "Your IP is unavailable",
			engine: "self_info",
			interactive: { type: "self_info", kind: "ip", value: ip },
		};
	}

	if (UA_QUERIES.has(q)) {
		return {
			answer: userAgent
				? `Your user-agent is: ${userAgent}`
				: "Your user-agent is unavailable",
			engine: "self_info",
			interactive: { type: "self_info", kind: "user_agent", value: userAgent },
		};
	}

	return null;
}
