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
	removeTimeFilter: string;
	searchGeneral: string;
	siteListUpdated: string;
	previousPage: string;
	nextPage: string;
	quotedPhrases: string;
	hideSite: string;
	preferSite: string;
	showMore: string;
	showLess: string;
	hideThisSite: string;
	imageUnavailable: string;
	mapNoCoords: string;
	timeRange: string;
	anyLanguageRegion: string;
	pagination: string;
	unknownPlace: string;
	currentConditions: string;
	forecast: string;
	tomorrowForecastMissing: string;
	currentNotSubstituted: string;
	sourcesCanApply: string;
	skippedSources: string;
	removeSite: string;
	hiddenSite: string;
	preferredSite: string;
	noPersonalSites: string;
	previousImage: string;
	nextImage: string;
	resolution: string;
	feelsLike: string;
	wind: string;
	humidity: string;
	activeConstraints: string;
	observed: string;
	tooManySearches: string;
	searchUnavailable: string;
	typeAQuery: string;
	rateLimitedHint: string;
	searchErrorHint: string;
	loadingResults: string;
	enginesDidntRespond: string;
	didYouMean: string;
	resultsRetrieved: string;
	noEngines: string;
	noEnginesHint: string;
	partialSources: string;
	hiddenAll: string;
	undo: string;
	showHidden: string;
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
	removeTimeFilter: "Remove time filter",
	searchGeneral: "Search the general category",
	siteListUpdated: "Site list updated.",
	previousPage: "Previous",
	nextPage: "Next",
	quotedPhrases:
		"Quoted phrases stay in the query for engines that support them. Zoeken does not check phrase order on retrieved pages.",
	hideSite: "Hide",
	preferSite: "Prefer",
	showMore: "Show more",
	showLess: "Show less",
	hideThisSite: "Hide site",
	imageUnavailable: "Image unavailable",
	mapNoCoords:
		"These results don’t include coordinates, so the map stays empty. The list below is still the full result set.",
	timeRange: "Time range",
	anyLanguageRegion: "Any language / region",
	pagination: "Pagination",
	unknownPlace: "Unknown place",
	currentConditions: "Current conditions",
	forecast: "Forecast",
	tomorrowForecastMissing: "Tomorrow’s forecast isn’t in this response",
	currentNotSubstituted: "Current conditions are not shown in its place.",
	sourcesCanApply:
		"{eligible} of {total} sources in this category can apply the active filters.",
	skippedSources: "Skipped {names} because they cannot honor them.",
	removeSite: "Remove",
	hiddenSite: "Hidden",
	preferredSite: "Preferred",
	noPersonalSites: "No personal site rules yet.",
	previousImage: "Previous image",
	nextImage: "Next image",
	resolution: "Resolution",
	feelsLike: "Feels like",
	wind: "Wind",
	humidity: "Humidity",
	activeConstraints: "Active search constraints",
	observed: "Observed",
	tooManySearches: "Too many searches",
	searchUnavailable: "Search is unavailable",
	typeAQuery: "Type a query to search.",
	rateLimitedHint: "Please wait a moment and try again.",
	searchErrorHint:
		"Something went wrong reaching the search backend. Try again in a moment.",
	loadingResults: "Loading results",
	enginesDidntRespond: "didn’t respond",
	didYouMean: "Did you mean",
	resultsRetrieved: "results retrieved",
	noEngines: "No sources can run this search",
	noEnginesHint:
		"The active filters leave no eligible engines. SafeSearch stays on until you change it.",
	partialSources: "Some sources didn’t respond",
	hiddenAll: "Hidden sites removed every result on this page.",
	undo: "Undo",
	showHidden: "Show hidden sites",
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
		removeTimeFilter: "Quitar el filtro de tiempo",
		searchGeneral: "Buscar en la categoría general",
		siteListUpdated: "Lista de sitios actualizada.",
		previousPage: "Anterior",
		nextPage: "Siguiente",
		quotedPhrases:
			"Las frases entre comillas se quedan en la consulta de los motores que las admiten. Zoeken no comprueba el orden en las páginas recuperadas.",
		hideSite: "Ocultar",
		preferSite: "Preferir",
		showMore: "Mostrar más",
		showLess: "Mostrar menos",
		hideThisSite: "Ocultar sitio",
		imageUnavailable: "Imagen no disponible",
		mapNoCoords:
			"Estos resultados no incluyen coordenadas, así que el mapa queda vacío. La lista de abajo sigue siendo el conjunto completo.",
		timeRange: "Intervalo de tiempo",
		anyLanguageRegion: "Cualquier idioma / región",
		pagination: "Paginación",
		unknownPlace: "Lugar desconocido",
		currentConditions: "Condiciones actuales",
		forecast: "Pronóstico",
		tomorrowForecastMissing:
			"El pronóstico de mañana no está en esta respuesta",
		currentNotSubstituted:
			"Las condiciones actuales no se muestran en su lugar.",
		sourcesCanApply:
			"{eligible} de {total} fuentes de esta categoría pueden aplicar los filtros activos.",
		skippedSources: "Se omitió {names} porque no pueden respetarlos.",
		removeSite: "Quitar",
		hiddenSite: "Oculto",
		preferredSite: "Preferido",
		noPersonalSites: "Todavía no hay reglas personales de sitios.",
		previousImage: "Imagen anterior",
		nextImage: "Imagen siguiente",
		resolution: "Resolución",
		feelsLike: "Sensación",
		wind: "Viento",
		humidity: "Humedad",
		activeConstraints: "Restricciones de búsqueda activas",
		observed: "Observado",
		tooManySearches: "Demasiadas búsquedas",
		searchUnavailable: "Búsqueda no disponible",
		enginesDidntRespond: "no respondieron",
		didYouMean: "Quizás quisiste decir",
		resultsRetrieved: "resultados obtenidos",
		noEngines: "Ninguna fuente puede hacer esta búsqueda",
		noEnginesHint:
			"Los filtros activos no dejan motores válidos. La búsqueda segura sigue activa hasta que la cambies.",
		partialSources: "Algunas fuentes no respondieron",
		hiddenAll:
			"Los sitios ocultos quitaron todos los resultados de esta página.",
		undo: "Deshacer",
		showHidden: "Mostrar sitios ocultos",
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
		removeTimeFilter: "Retirer le filtre de date",
		searchGeneral: "Chercher dans la catégorie générale",
		siteListUpdated: "Liste de sites mise à jour.",
		previousPage: "Précédent",
		nextPage: "Suivant",
		quotedPhrases:
			"Les phrases entre guillemets restent dans la requête des moteurs qui les acceptent. Zoeken ne vérifie pas l’ordre sur les pages récupérées.",
		hideSite: "Masquer",
		preferSite: "Préférer",
		showMore: "Afficher plus",
		showLess: "Afficher moins",
		hideThisSite: "Masquer le site",
		imageUnavailable: "Image indisponible",
		mapNoCoords:
			"Ces résultats n’incluent pas de coordonnées, donc la carte reste vide. La liste ci-dessous reste l’ensemble complet.",
		timeRange: "Période",
		anyLanguageRegion: "Toute langue / région",
		pagination: "Pagination",
		unknownPlace: "Lieu inconnu",
		currentConditions: "Conditions actuelles",
		forecast: "Prévision",
		tomorrowForecastMissing:
			"La prévision de demain n’est pas dans cette réponse",
		currentNotSubstituted:
			"Les conditions actuelles ne sont pas affichées à sa place.",
		sourcesCanApply:
			"{eligible} source(s) sur {total} dans cette catégorie peuvent appliquer les filtres actifs.",
		skippedSources:
			"{names} ignoré, car ces sources ne peuvent pas les respecter.",
		removeSite: "Retirer",
		hiddenSite: "Masqué",
		preferredSite: "Préféré",
		noPersonalSites: "Aucune règle personnelle de site pour l’instant.",
		previousImage: "Image précédente",
		nextImage: "Image suivante",
		resolution: "Résolution",
		feelsLike: "Ressenti",
		wind: "Vent",
		humidity: "Humidité",
		activeConstraints: "Contraintes de recherche actives",
		observed: "Observé",
		tooManySearches: "Trop de recherches",
		searchUnavailable: "Recherche indisponible",
		enginesDidntRespond: "n’ont pas répondu",
		didYouMean: "Vouliez-vous dire",
		resultsRetrieved: "résultats récupérés",
		noEngines: "Aucune source ne peut lancer cette recherche",
		noEnginesHint:
			"Les filtres actifs ne laissent aucun moteur éligible. La recherche sécurisée reste active tant que vous ne la changez pas.",
		partialSources: "Certaines sources n’ont pas répondu",
		hiddenAll: "Les sites masqués ont retiré tous les résultats de cette page.",
		undo: "Annuler",
		showHidden: "Afficher les sites masqués",
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
		removeTimeFilter: "Zeitfilter entfernen",
		searchGeneral: "In der allgemeinen Kategorie suchen",
		siteListUpdated: "Seitenliste aktualisiert.",
		previousPage: "Zurück",
		nextPage: "Weiter",
		quotedPhrases:
			"Zitierte Phrasen bleiben in der Anfrage der Suchmaschinen, die sie unterstützen. Zoeken prüft die Reihenfolge nicht an den abgerufenen Seiten.",
		hideSite: "Ausblenden",
		preferSite: "Bevorzugen",
		showMore: "Mehr anzeigen",
		showLess: "Weniger anzeigen",
		hideThisSite: "Seite ausblenden",
		imageUnavailable: "Bild nicht verfügbar",
		mapNoCoords:
			"Diese Ergebnisse enthalten keine Koordinaten, daher bleibt die Karte leer. Die Liste darunter ist weiterhin die volle Trefferliste.",
		timeRange: "Zeitraum",
		anyLanguageRegion: "Beliebige Sprache / Region",
		pagination: "Seiten",
		unknownPlace: "Unbekannter Ort",
		currentConditions: "Aktuelle Bedingungen",
		forecast: "Vorhersage",
		tomorrowForecastMissing:
			"Die Vorhersage für morgen fehlt in dieser Antwort",
		currentNotSubstituted:
			"Die aktuellen Bedingungen werden nicht an ihrer Stelle gezeigt.",
		sourcesCanApply:
			"{eligible} von {total} Quellen in dieser Kategorie können die aktiven Filter anwenden.",
		skippedSources:
			"{names} übersprungen, weil sie die Filter nicht einhalten können.",
		removeSite: "Entfernen",
		hiddenSite: "Ausgeblendet",
		preferredSite: "Bevorzugt",
		noPersonalSites: "Noch keine persönlichen Seitenregeln.",
		previousImage: "Vorheriges Bild",
		nextImage: "Nächstes Bild",
		resolution: "Auflösung",
		feelsLike: "Gefühlt",
		wind: "Wind",
		humidity: "Luftfeuchtigkeit",
		activeConstraints: "Aktive Sucheinschränkungen",
		observed: "Beobachtet",
		tooManySearches: "Zu viele Suchanfragen",
		searchUnavailable: "Suche nicht verfügbar",
		enginesDidntRespond: "haben nicht geantwortet",
		didYouMean: "Meintest du",
		resultsRetrieved: "abgerufene Ergebnisse",
		noEngines: "Keine Quelle kann diese Suche ausführen",
		noEnginesHint:
			"Die aktiven Filter lassen keine passende Suchmaschine übrig. SafeSearch bleibt an, bis du es änderst.",
		partialSources: "Einige Quellen haben nicht geantwortet",
		hiddenAll:
			"Ausgeblendete Seiten haben alle Ergebnisse dieser Seite entfernt.",
		undo: "Rückgängig",
		showHidden: "Ausgeblendete Seiten zeigen",
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
		removeTimeFilter: "Tijdfilter verwijderen",
		searchGeneral: "In de algemene categorie zoeken",
		siteListUpdated: "Sitelijst bijgewerkt.",
		previousPage: "Vorige",
		nextPage: "Volgende",
		quotedPhrases:
			"Geciteerde zinnen blijven in de zoekopdracht van engines die ze ondersteunen. Zoeken controleert de volgorde niet op de opgehaalde pagina’s.",
		hideSite: "Verbergen",
		preferSite: "Voorkeur",
		showMore: "Meer tonen",
		showLess: "Minder tonen",
		hideThisSite: "Site verbergen",
		imageUnavailable: "Afbeelding niet beschikbaar",
		mapNoCoords:
			"Deze resultaten bevatten geen coördinaten, dus de kaart blijft leeg. De lijst eronder is nog steeds de volledige resultaatset.",
		timeRange: "Tijdsperiode",
		anyLanguageRegion: "Elke taal / regio",
		pagination: "Paginering",
		unknownPlace: "Onbekende plaats",
		currentConditions: "Huidige omstandigheden",
		forecast: "Verwachting",
		tomorrowForecastMissing:
			"De verwachting voor morgen staat niet in dit antwoord",
		currentNotSubstituted:
			"De huidige omstandigheden worden niet in de plaats daarvan getoond.",
		sourcesCanApply:
			"{eligible} van {total} bronnen in deze categorie kunnen de actieve filters toepassen.",
		skippedSources:
			"{names} overgeslagen omdat ze de filters niet kunnen volgen.",
		removeSite: "Verwijderen",
		hiddenSite: "Verborgen",
		preferredSite: "Voorkeur",
		noPersonalSites: "Nog geen persoonlijke siteregels.",
		previousImage: "Vorige afbeelding",
		nextImage: "Volgende afbeelding",
		resolution: "Resolutie",
		feelsLike: "Gevoelstemperatuur",
		wind: "Wind",
		humidity: "Luchtvochtigheid",
		activeConstraints: "Actieve zoekbeperkingen",
		observed: "Waargenomen",
		tooManySearches: "Te veel zoekopdrachten",
		searchUnavailable: "Zoeken niet beschikbaar",
		enginesDidntRespond: "reageerden niet",
		didYouMean: "Bedoelde je",
		resultsRetrieved: "opgehaalde resultaten",
		noEngines: "Geen bron kan deze zoekopdracht uitvoeren",
		noEnginesHint:
			"De actieve filters laten geen geschikte zoekmachines over. SafeSearch blijft aan tot je het wijzigt.",
		partialSources: "Sommige bronnen reageerden niet",
		hiddenAll:
			"Verborgen sites hebben alle resultaten op deze pagina verwijderd.",
		undo: "Ongedaan maken",
		showHidden: "Verborgen sites tonen",
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

/** Strings for the language currently in the page URL. */
export function activeStrings(): UiStrings {
	if (typeof window === "undefined") return stringsFor(undefined);
	const language = new URLSearchParams(window.location.search).get("language");
	return stringsFor(language ?? undefined);
}
