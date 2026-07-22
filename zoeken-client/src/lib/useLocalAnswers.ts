/** Calculator/time/self-info/statistics/random/date-math/crypto answers, computed locally. */
import { useMemo } from "react";
import type { Config, Preferences, SearchAnswer } from "./api";
import { pluginEnabled } from "./clientFeatures";
import { computeCalculatorAnswer } from "./clientFeatures/calculator";
import { computeCryptoAnswer } from "./clientFeatures/crypto";
import { computeDateTimeAnswer } from "./clientFeatures/dateTime";
import { computeRandomAnswer } from "./clientFeatures/random";
import { computeSelfInfoAnswer } from "./clientFeatures/selfInfo";
import { computeStatisticsAnswer } from "./clientFeatures/statistics";
import { computeTimeZoneAnswer } from "./clientFeatures/timeZone";
import { computeUnitConverterAnswer } from "./clientFeatures/unitConverter";

export function useLocalAnswers(
	q: string,
	language: string | undefined,
	pageno: number,
	config: Config | undefined,
	prefs?: Preferences | null,
): SearchAnswer[] {
	// biome-ignore lint/correctness/useExhaustiveDependencies: navigator.userAgent is stable per session
	return useMemo(() => {
		const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
		return [
			pluginEnabled(config, "calculator", prefs)
				? computeCalculatorAnswer(q, language ?? "", pageno)
				: null,
			pluginEnabled(config, "time_zone", prefs)
				? computeTimeZoneAnswer(q, pageno)
				: null,
			pluginEnabled(config, "self_info", prefs)
				? computeSelfInfoAnswer(q, pageno, config?.client_ip ?? null, ua)
				: null,
			pluginEnabled(config, "unit_converter", prefs)
				? computeUnitConverterAnswer(q, pageno)
				: null,
			computeStatisticsAnswer(q),
			computeRandomAnswer(q),
			computeDateTimeAnswer(q),
			computeCryptoAnswer(q),
		].filter((answer) => answer !== null);
	}, [q, language, pageno, config, prefs]);
}
