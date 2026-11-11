import { describe, expect, it } from "vitest";
import { presentAnswers, weatherPresentation } from "./answers";
import type { SearchAnswer } from "./api";
import { computeDateTimeAnswer } from "./clientFeatures/dateTime";

function answer(engine: string, type?: string): SearchAnswer {
	return {
		answer: engine,
		url: null,
		engine,
		interactive: type
			? ({
					type,
					place: "",
					description: "",
					temp_c: "",
					temp_f: "",
					feels_c: "",
					wind_kmph: "",
					wind_dir: "",
					humidity: "",
					observation_time: "",
					forecast_date: "",
					forecast_summary: "",
					forecast_temp_c: "",
					forecast_temp_f: "",
				} as SearchAnswer["interactive"])
			: null,
	};
}

describe("answer presentation", () => {
	it("keeps one calculator when local and remote both answer", () => {
		const local = answer("calculator", "calculator");
		const remote = answer("calculator", "calculator");
		const shown = presentAnswers([local], [remote], "12 * 8");
		expect(shown).toEqual([local]);
	});

	it("keeps a date answer that the intent guess does not name", () => {
		const date = computeDateTimeAnswer("days until 2027-01-01");
		expect(date).not.toBeNull();
		const shown = presentAnswers(
			date ? [date] : [],
			[],
			"days until 2027-01-01",
			"arithmetic",
		);
		expect(shown).toEqual(date ? [date] : []);
	});

	it("keeps an eligible offline answer when the predicted kind is absent", () => {
		const calculator = answer("calculator", "calculator");
		expect(
			presentAnswers([calculator], [], "weather in paris", "weather"),
		).toEqual([calculator]);
		const unit = answer("unit", "unit");
		expect(presentAnswers([unit], [], "define rust", "definition")).toEqual([
			unit,
		]);
	});

	it("does not present current weather as tomorrow when no forecast arrived", () => {
		expect(weatherPresentation("weather tomorrow in paris", "", "")).toBe(
			"unavailable",
		);
		expect(weatherPresentation("weather tomorrow in paris", "9", "Rain")).toBe(
			"forecast",
		);
		expect(weatherPresentation("weather in paris", "18", "")).toBe("current");
	});
});
