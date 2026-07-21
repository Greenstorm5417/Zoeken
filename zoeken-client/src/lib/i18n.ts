/** Lightweight UI translations keyed by the SPA's own language preference.
 *
 * Not a full i18n framework — a flat dictionary covering the chrome the user
 * sees most (nav, filters, states). Unknown keys/locales fall back to English.
 */

export type UiStrings = {
	preferences: string;
	about: string;
	anyTime: string;
	pastDay: string;
	pastWeek: string;
	pastMonth: string;
	pastYear: string;
	anyLanguage: string;
	safeSearchOff: string;
	moderate: string;
	strict: string;
	searching: string;
	noResults: string;
	tryDifferent: string;
	relatedSearches: string;
	loadMore: string;
	endOfResults: string;
	retry: string;
	tooManySearches: string;
	searchUnavailable: string;
	enginesDidntRespond: string;
	didYouMean: string;
};

const EN: UiStrings = {
	preferences: "Preferences",
	about: "About",
	anyTime: "Any time",
	pastDay: "Past day",
	pastWeek: "Past week",
	pastMonth: "Past month",
	pastYear: "Past year",
	anyLanguage: "Any language",
	safeSearchOff: "SafeSearch off",
	moderate: "Moderate",
	strict: "Strict",
	searching: "Searching…",
	noResults: "No results",
	tryDifferent:
		"Try different keywords, another category tab, or fewer filters.",
	relatedSearches: "Related searches",
	loadMore: "Load more",
	endOfResults: "End of results",
	retry: "Retry",
	tooManySearches: "Too many searches",
	searchUnavailable: "Search is unavailable",
	enginesDidntRespond: "didn’t respond",
	didYouMean: "Did you mean",
};

const DICTS: Record<string, Partial<UiStrings>> = {
	es: {
		preferences: "Preferencias",
		about: "Acerca de",
		anyTime: "Cualquier momento",
		pastDay: "Último día",
		pastWeek: "Última semana",
		pastMonth: "Último mes",
		pastYear: "Último año",
		anyLanguage: "Cualquier idioma",
		safeSearchOff: "Sin filtro",
		moderate: "Moderado",
		strict: "Estricto",
		searching: "Buscando…",
		noResults: "Sin resultados",
		tryDifferent:
			"Prueba otras palabras clave, otra categoría o menos filtros.",
		relatedSearches: "Búsquedas relacionadas",
		loadMore: "Cargar más",
		endOfResults: "Fin de los resultados",
		retry: "Reintentar",
		tooManySearches: "Demasiadas búsquedas",
		searchUnavailable: "Búsqueda no disponible",
		enginesDidntRespond: "no respondieron",
		didYouMean: "Quizás quisiste decir",
	},
	fr: {
		preferences: "Préférences",
		about: "À propos",
		anyTime: "N’importe quand",
		pastDay: "Dernier jour",
		pastWeek: "Dernière semaine",
		pastMonth: "Dernier mois",
		pastYear: "Dernière année",
		anyLanguage: "Toutes les langues",
		safeSearchOff: "Filtre désactivé",
		moderate: "Modéré",
		strict: "Strict",
		searching: "Recherche…",
		noResults: "Aucun résultat",
		tryDifferent:
			"Essayez d’autres mots-clés, une autre catégorie ou moins de filtres.",
		relatedSearches: "Recherches associées",
		loadMore: "Charger plus",
		endOfResults: "Fin des résultats",
		retry: "Réessayer",
		tooManySearches: "Trop de recherches",
		searchUnavailable: "Recherche indisponible",
		enginesDidntRespond: "n’ont pas répondu",
		didYouMean: "Vouliez-vous dire",
	},
	de: {
		preferences: "Einstellungen",
		about: "Über",
		anyTime: "Jederzeit",
		pastDay: "Letzter Tag",
		pastWeek: "Letzte Woche",
		pastMonth: "Letzter Monat",
		pastYear: "Letztes Jahr",
		anyLanguage: "Alle Sprachen",
		safeSearchOff: "SafeSearch aus",
		moderate: "Moderat",
		strict: "Streng",
		searching: "Suche…",
		noResults: "Keine Ergebnisse",
		tryDifferent:
			"Versuche andere Suchbegriffe, eine andere Kategorie oder weniger Filter.",
		relatedSearches: "Ähnliche Suchen",
		loadMore: "Mehr laden",
		endOfResults: "Ende der Ergebnisse",
		retry: "Erneut versuchen",
		tooManySearches: "Zu viele Suchanfragen",
		searchUnavailable: "Suche nicht verfügbar",
		enginesDidntRespond: "haben nicht geantwortet",
		didYouMean: "Meintest du",
	},
	nl: {
		preferences: "Voorkeuren",
		about: "Over",
		anyTime: "Elk moment",
		pastDay: "Afgelopen dag",
		pastWeek: "Afgelopen week",
		pastMonth: "Afgelopen maand",
		pastYear: "Afgelopen jaar",
		anyLanguage: "Elke taal",
		safeSearchOff: "SafeSearch uit",
		moderate: "Gematigd",
		strict: "Streng",
		searching: "Zoeken…",
		noResults: "Geen resultaten",
		tryDifferent:
			"Probeer andere zoekwoorden, een andere categorie of minder filters.",
		relatedSearches: "Gerelateerde zoekopdrachten",
		loadMore: "Meer laden",
		endOfResults: "Einde van de resultaten",
		retry: "Opnieuw",
		tooManySearches: "Te veel zoekopdrachten",
		searchUnavailable: "Zoeken niet beschikbaar",
		enginesDidntRespond: "reageerden niet",
		didYouMean: "Bedoelde je",
	},
};

/** Resolve the string table for a locale/language tag (e.g. `es`, `fr-FR`). */
export function stringsFor(locale: string | undefined): UiStrings {
	if (!locale) return EN;
	const lang = locale.toLowerCase().split(/[-_]/)[0];
	const dict = DICTS[lang];
	return dict ? { ...EN, ...dict } : EN;
}
