/** Lightweight UI translations keyed by the SPA's own language preference.
 *
 * Not a full i18n framework — a flat dictionary covering the chrome the user
 * sees most (nav, filters, prefs/about/stats). Unknown keys/locales fall back
 * to English. Engine descriptions stay untranslated on purpose.
 */

export type UiStrings = {
	preferences: string;
	about: string;
	stats: string;
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
	typeAQuery: string;
	rateLimitedHint: string;
	searchErrorHint: string;
	loadingResults: string;
	enginesDidntRespond: string;
	didYouMean: string;
	// Preferences chrome
	prefsSavedLocally: string;
	prefsSearch: string;
	prefsAppearance: string;
	prefsInterfaceLanguage: string;
	prefsSearchLanguage: string;
	prefsSearchMethod: string;
	prefsSafeSearch: string;
	prefsAutocomplete: string;
	prefsTheme: string;
	prefsThemeHint: string;
	prefsSync: string;
	prefsSyncHint: string;
	prefsCopyLink: string;
	prefsCopied: string;
	prefsLoading: string;
	prefsUnavailable: string;
	prefsAuto: string;
	prefsOff: string;
	// About
	aboutBlurb: string;
	aboutDocs: string;
	aboutPrivacy: string;
	aboutContact: string;
	aboutSource: string;
	aboutVersion: string;
	// Stats
	statsTitle: string;
	statsBlurb: string;
	statsTiming: string;
	statsErrors: string;
	statsLoading: string;
	statsNoSamples: string;
	statsNoErrors: string;
	statsEngine: string;
	statsStatus: string;
	statsEnabled: string;
	statsDisabled: string;
	statsSuspended: string;
	statsSuspendedUntil: string;
	statsRequests: string;
	statsAvgMs: string;
	statsHttpAvgMs: string;
	statsCouldntLoadTiming: string;
	statsCouldntLoadErrors: string;
	statsTotal: string;
};

const EN: UiStrings = {
	preferences: "Preferences",
	about: "About",
	stats: "Stats",
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
	typeAQuery: "Type a query to search.",
	rateLimitedHint: "Please wait a moment and try again.",
	searchErrorHint:
		"Something went wrong reaching the search backend. Try again in a moment.",
	loadingResults: "Loading results",
	enginesDidntRespond: "didn’t respond",
	didYouMean: "Did you mean",
	prefsSavedLocally: "Changes are saved to this browser.",
	prefsSearch: "Search",
	prefsAppearance: "Appearance",
	prefsInterfaceLanguage: "Interface language",
	prefsSearchLanguage: "Search language",
	prefsSearchMethod: "Search method",
	prefsSafeSearch: "Safe search",
	prefsAutocomplete: "Autocomplete",
	prefsTheme: "Theme",
	prefsThemeHint:
		"Stored in this browser, independent of your search settings.",
	prefsSync: "Sync settings",
	prefsSyncHint:
		"Copy a link that carries these settings — open it in another browser or on another device to apply them, no account needed.",
	prefsCopyLink: "Copy settings link",
	prefsCopied: "Copied",
	prefsLoading: "Loading preferences…",
	prefsUnavailable: "Preferences are unavailable.",
	prefsAuto: "Auto",
	prefsOff: "Off",
	aboutBlurb:
		"A clean, private metasearch experience that brings results together without tracking your searches.",
	aboutDocs: "Documentation",
	aboutPrivacy: "Privacy policy",
	aboutContact: "Contact",
	aboutSource: "Source code",
	aboutVersion: "Version",
	statsTitle: "Engine stats",
	statsBlurb:
		"Response timing, errors, and whether each engine is disabled or suspended.",
	statsTiming: "Timing",
	statsErrors: "Errors",
	statsLoading: "Loading…",
	statsNoSamples: "No engines registered on this instance.",
	statsNoErrors: "No recorded engine errors.",
	statsEngine: "Engine",
	statsStatus: "Status",
	statsEnabled: "Enabled",
	statsDisabled: "Disabled",
	statsSuspended: "Suspended",
	statsSuspendedUntil: "Suspended until",
	statsRequests: "Requests",
	statsAvgMs: "Avg (ms)",
	statsHttpAvgMs: "HTTP avg (ms)",
	statsCouldntLoadTiming: "Couldn’t load timing stats.",
	statsCouldntLoadErrors: "Couldn’t load error stats.",
	statsTotal: "total",
};

const DICTS: Record<string, Partial<UiStrings>> = {
	es: {
		preferences: "Preferencias",
		about: "Acerca de",
		stats: "Estadísticas",
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
		prefsSavedLocally: "Los cambios se guardan en este navegador.",
		prefsSearch: "Búsqueda",
		prefsAppearance: "Apariencia",
		prefsInterfaceLanguage: "Idioma de la interfaz",
		prefsSearchLanguage: "Idioma de búsqueda",
		prefsSearchMethod: "Método de búsqueda",
		prefsSafeSearch: "Búsqueda segura",
		prefsAutocomplete: "Autocompletado",
		prefsTheme: "Tema",
		prefsThemeHint:
			"Se guarda en este navegador, independiente de tus ajustes de búsqueda.",
		prefsSync: "Sincronizar ajustes",
		prefsSyncHint:
			"Copia un enlace con estos ajustes — ábrelo en otro navegador o dispositivo para aplicarlos, sin cuenta.",
		prefsCopyLink: "Copiar enlace de ajustes",
		prefsCopied: "Copiado",
		prefsLoading: "Cargando preferencias…",
		prefsUnavailable: "Preferencias no disponibles.",
		prefsAuto: "Auto",
		prefsOff: "Desactivado",
		aboutBlurb:
			"Una experiencia de metabúsqueda limpia y privada que reúne resultados sin rastrear tus búsquedas.",
		aboutDocs: "Documentación",
		aboutPrivacy: "Política de privacidad",
		aboutContact: "Contacto",
		aboutSource: "Código fuente",
		aboutVersion: "Versión",
		statsTitle: "Estadísticas de motores",
		statsBlurb:
			"Tiempos de respuesta, errores y si cada motor está desactivado o suspendido.",
		statsTiming: "Tiempos",
		statsErrors: "Errores",
		statsLoading: "Cargando…",
		statsNoSamples: "No hay motores registrados en esta instancia.",
		statsNoErrors: "No hay errores registrados.",
		statsEngine: "Motor",
		statsStatus: "Estado",
		statsEnabled: "Activo",
		statsDisabled: "Desactivado",
		statsSuspended: "Suspendido",
		statsSuspendedUntil: "Suspendido hasta",
		statsRequests: "Peticiones",
		statsAvgMs: "Media (ms)",
		statsHttpAvgMs: "HTTP media (ms)",
		statsCouldntLoadTiming: "No se pudieron cargar los tiempos.",
		statsCouldntLoadErrors: "No se pudieron cargar los errores.",
		statsTotal: "en total",
	},
	fr: {
		preferences: "Préférences",
		about: "À propos",
		stats: "Stats",
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
		prefsSavedLocally:
			"Les modifications sont enregistrées dans ce navigateur.",
		prefsSearch: "Recherche",
		prefsAppearance: "Apparence",
		prefsInterfaceLanguage: "Langue de l’interface",
		prefsSearchLanguage: "Langue de recherche",
		prefsSearchMethod: "Méthode de recherche",
		prefsSafeSearch: "Recherche sécurisée",
		prefsAutocomplete: "Autocomplétion",
		prefsTheme: "Thème",
		prefsThemeHint:
			"Enregistré dans ce navigateur, indépendamment de vos paramètres de recherche.",
		prefsSync: "Synchroniser les réglages",
		prefsSyncHint:
			"Copiez un lien portant ces réglages — ouvrez-le dans un autre navigateur ou appareil pour les appliquer, sans compte.",
		prefsCopyLink: "Copier le lien des réglages",
		prefsCopied: "Copié",
		prefsLoading: "Chargement des préférences…",
		prefsUnavailable: "Préférences indisponibles.",
		prefsAuto: "Auto",
		prefsOff: "Désactivé",
		aboutBlurb:
			"Une expérience de métarecherche propre et privée qui regroupe les résultats sans suivre vos recherches.",
		aboutDocs: "Documentation",
		aboutPrivacy: "Politique de confidentialité",
		aboutContact: "Contact",
		aboutSource: "Code source",
		aboutVersion: "Version",
		statsTitle: "Stats des moteurs",
		statsBlurb:
			"Temps de réponse, erreurs, et si chaque moteur est désactivé ou suspendu.",
		statsTiming: "Temps",
		statsErrors: "Erreurs",
		statsLoading: "Chargement…",
		statsNoSamples: "Aucun moteur enregistré sur cette instance.",
		statsNoErrors: "Aucune erreur enregistrée.",
		statsEngine: "Moteur",
		statsStatus: "État",
		statsEnabled: "Actif",
		statsDisabled: "Désactivé",
		statsSuspended: "Suspendu",
		statsSuspendedUntil: "Suspendu jusqu’au",
		statsRequests: "Requêtes",
		statsAvgMs: "Moy. (ms)",
		statsHttpAvgMs: "HTTP moy. (ms)",
		statsCouldntLoadTiming: "Impossible de charger les temps.",
		statsCouldntLoadErrors: "Impossible de charger les erreurs.",
		statsTotal: "au total",
	},
	de: {
		preferences: "Einstellungen",
		about: "Über",
		stats: "Statistik",
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
		prefsSavedLocally: "Änderungen werden in diesem Browser gespeichert.",
		prefsSearch: "Suche",
		prefsAppearance: "Darstellung",
		prefsInterfaceLanguage: "Oberflächensprache",
		prefsSearchLanguage: "Suchsprache",
		prefsSearchMethod: "Suchmethode",
		prefsSafeSearch: "SafeSearch",
		prefsAutocomplete: "Autovervollständigung",
		prefsTheme: "Design",
		prefsThemeHint:
			"Wird in diesem Browser gespeichert, unabhängig von den Sucheinstellungen.",
		prefsSync: "Einstellungen synchronisieren",
		prefsSyncHint:
			"Kopiere einen Link mit diesen Einstellungen — öffne ihn in einem anderen Browser oder Gerät, ohne Konto.",
		prefsCopyLink: "Einstellungslink kopieren",
		prefsCopied: "Kopiert",
		prefsLoading: "Einstellungen werden geladen…",
		prefsUnavailable: "Einstellungen nicht verfügbar.",
		prefsAuto: "Auto",
		prefsOff: "Aus",
		aboutBlurb:
			"Eine saubere, private Metasuche, die Ergebnisse zusammenführt, ohne deine Suchen zu verfolgen.",
		aboutDocs: "Dokumentation",
		aboutPrivacy: "Datenschutz",
		aboutContact: "Kontakt",
		aboutSource: "Quellcode",
		aboutVersion: "Version",
		statsTitle: "Motoren-Statistik",
		statsBlurb:
			"Antwortzeiten, Fehler und ob jeder Motor deaktiviert oder gesperrt ist.",
		statsTiming: "Zeiten",
		statsErrors: "Fehler",
		statsLoading: "Laden…",
		statsNoSamples: "Keine Motoren auf dieser Instanz registriert.",
		statsNoErrors: "Keine aufgezeichneten Fehler.",
		statsEngine: "Motor",
		statsStatus: "Status",
		statsEnabled: "Aktiv",
		statsDisabled: "Deaktiviert",
		statsSuspended: "Gesperrt",
		statsSuspendedUntil: "Gesperrt bis",
		statsRequests: "Anfragen",
		statsAvgMs: "Ø (ms)",
		statsHttpAvgMs: "HTTP Ø (ms)",
		statsCouldntLoadTiming: "Zeiten konnten nicht geladen werden.",
		statsCouldntLoadErrors: "Fehler konnten nicht geladen werden.",
		statsTotal: "gesamt",
	},
	nl: {
		preferences: "Voorkeuren",
		about: "Over",
		stats: "Statistieken",
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
		prefsSavedLocally: "Wijzigingen worden in deze browser opgeslagen.",
		prefsSearch: "Zoeken",
		prefsAppearance: "Weergave",
		prefsInterfaceLanguage: "Interfacetaal",
		prefsSearchLanguage: "Zoektaal",
		prefsSearchMethod: "Zoekmethode",
		prefsSafeSearch: "Veilig zoeken",
		prefsAutocomplete: "Automatisch aanvullen",
		prefsTheme: "Thema",
		prefsThemeHint: "Opgeslagen in deze browser, los van je zoekinstellingen.",
		prefsSync: "Instellingen synchroniseren",
		prefsSyncHint:
			"Kopieer een link met deze instellingen — open die in een andere browser of op een ander apparaat, zonder account.",
		prefsCopyLink: "Instellingenlink kopiëren",
		prefsCopied: "Gekopieerd",
		prefsLoading: "Voorkeuren laden…",
		prefsUnavailable: "Voorkeuren niet beschikbaar.",
		prefsAuto: "Auto",
		prefsOff: "Uit",
		aboutBlurb:
			"Een schone, private metazoekervaring die resultaten samenbrengt zonder je zoekopdrachten te volgen.",
		aboutDocs: "Documentatie",
		aboutPrivacy: "Privacybeleid",
		aboutContact: "Contact",
		aboutSource: "Broncode",
		aboutVersion: "Versie",
		statsTitle: "Motorstatistieken",
		statsBlurb:
			"Responstijden, fouten en of elke motor uitgeschakeld of geschorst is.",
		statsTiming: "Timing",
		statsErrors: "Fouten",
		statsLoading: "Laden…",
		statsNoSamples: "Geen motoren geregistreerd op deze instantie.",
		statsNoErrors: "Geen geregistreerde motorfouten.",
		statsEngine: "Motor",
		statsStatus: "Status",
		statsEnabled: "Ingeschakeld",
		statsDisabled: "Uitgeschakeld",
		statsSuspended: "Geschorst",
		statsSuspendedUntil: "Geschorst tot",
		statsRequests: "Verzoeken",
		statsAvgMs: "Gem. (ms)",
		statsHttpAvgMs: "HTTP gem. (ms)",
		statsCouldntLoadTiming: "Timing kon niet worden geladen.",
		statsCouldntLoadErrors: "Fouten konden niet worden geladen.",
		statsTotal: "totaal",
	},
};

/** Resolve the string table for a locale/language tag (e.g. `es`, `fr-FR`). */
export function stringsFor(locale: string | undefined): UiStrings {
	if (!locale) return EN;
	const lang = locale.toLowerCase().split(/[-_]/)[0];
	const dict = DICTS[lang];
	return dict ? { ...EN, ...dict } : EN;
}
