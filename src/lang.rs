//! Per-language word packs for prompt linting.
//!
//! All entries are written in *folded* form (see [`fold`]): lowercase, accents
//! removed, German umlauts transliterated (`ä` → `ae`), Greek final sigma
//! normalized. An entry ending in `*` matches any word with that prefix,
//! which covers inflected forms (`διορθ*` → διόρθωσε, διορθώστε, …).
//! Entries containing a space are phrases, matched on whole words.
//!
//! Prompts are linted with English plus the detected language, because
//! engineers mix English technical terms into every language.

/// A language's vocabulary for the lint rules.
pub struct Lang {
    pub code: &'static str,
    /// Verbs that make a prompt a request to change something.
    pub task_verbs: &'static [&'static str],
    /// Symptom statements or verb phrases that imply a task ("the build is red").
    pub task_phrases: &'static [&'static str],
    /// Words that don't identify *what* to change.
    pub generic: &'static [&'static str],
    /// References to earlier conversation: the target is already established.
    pub continuations: &'static [&'static str],
    /// First words of a question.
    pub question_words: &'static [&'static str],
    /// Words that state what "done" looks like.
    pub done_markers: &'static [&'static str],
    /// Words meaning "line" (as in "line 42").
    pub line_words: &'static [&'static str],
    /// Frequent function words used to detect the language.
    pub markers: &'static [&'static str],
    pub messages: Messages,
}

pub struct Messages {
    pub vague_target: &'static str,
    /// `{n}` is replaced with the token estimate.
    pub large_paste: &'static str,
    pub no_done_criteria: &'static str,
    /// Header when the hook holds a prompt; `{codes}` lists the findings.
    pub held: &'static str,
    /// How to proceed after a hold.
    pub hold_hint: &'static str,
}

/// Lowercase and strip diacritics so word lists don't need every spelling.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'á' | 'à' | 'â' | 'ã' | 'å' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'ó' | 'ò' | 'ô' | 'õ' => out.push('o'),
            'ú' | 'ù' | 'û' => out.push('u'),
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ά' => out.push('α'),
            'έ' => out.push('ε'),
            'ή' => out.push('η'),
            'ί' | 'ϊ' | 'ΐ' => out.push('ι'),
            'ό' => out.push('ο'),
            'ύ' | 'ϋ' | 'ΰ' => out.push('υ'),
            'ώ' => out.push('ω'),
            'ς' => out.push('σ'),
            '’' | '‘' | 'ʼ' => out.push('\''),
            c => out.push(c),
        }
    }
    out
}

/// Does a word-list entry match this (folded) word?
pub fn entry_matches(entry: &str, word: &str) -> bool {
    match entry.strip_suffix('*') {
        Some(prefix) => word.starts_with(prefix),
        None => entry == word,
    }
}

/// Does any entry match one of `words` (or, for phrases, appear in `phrase`,
/// which is the words joined by single spaces and padded with spaces)?
pub fn mentions(list: &[&str], words: &[String], phrase: &str) -> bool {
    list.iter().any(|entry| {
        if entry.contains(' ') {
            phrase.contains(&format!(" {entry} "))
        } else {
            words.iter().any(|w| entry_matches(entry, w))
        }
    })
}

pub fn list_has(list: &[&str], word: &str) -> bool {
    list.iter().any(|entry| entry_matches(entry, word))
}

/// Evidence that a prompt is in `lang`: function words and task verbs.
fn evidence(lang: &Lang, folded_words: &[String]) -> usize {
    folded_words
        .iter()
        .filter(|w| list_has(lang.markers, w) || list_has(lang.task_verbs, w))
        .count()
}

/// The languages a prompt is linted in: English (technical vocabulary is
/// English everywhere) plus every language with evidence in the prompt. The
/// first entry is the prompt's main language, used for messages: Greek by
/// script, otherwise the strongest non-English evidence that beats English.
pub fn detect(folded_words: &[String], raw: &str) -> Vec<&'static Lang> {
    let greek = raw
        .chars()
        .filter(|c| ('\u{0370}'..='\u{03ff}').contains(c) || ('\u{1f00}'..='\u{1fff}').contains(c))
        .count();
    let latin = raw.chars().filter(char::is_ascii_alphabetic).count();
    // Engineers mix English terms into Greek prompts; a third is plenty of evidence.
    if greek > 0 && greek * 3 >= latin {
        return vec![&EL, &EN];
    }
    let english = evidence(&EN, folded_words);
    let mut others: Vec<(usize, &'static Lang)> = [&FR, &ES, &DE]
        .into_iter()
        .map(|lang| (evidence(lang, folded_words), lang))
        .filter(|(score, _)| *score > 0)
        .collect();
    others.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    let mut packs = Vec::new();
    if let Some((best, lang)) = others.first() {
        if *best > english {
            packs.push(*lang);
        }
    }
    packs.push(&EN);
    for (_, lang) in others {
        if !packs.iter().any(|p| p.code == lang.code) {
            packs.push(lang);
        }
    }
    packs
}

pub static ALL: [&Lang; 5] = [&EN, &EL, &FR, &ES, &DE];

#[rustfmt::skip]
pub static EN: Lang = Lang {
    code: "en",
    task_verbs: &[
        "fix", "add", "implement", "refactor", "debug", "update", "change", "make", "create",
        "write", "build", "investigate", "optimize", "optimise", "improve", "remove", "delete",
        "rename", "migrate", "support", "handle", "resolve", "clean", "speed",
    ],
    task_phrases: &[
        "broken", "failing", "fails", "crashes", "crashing", "red", "throws", "not working",
        "doesn't work", "isn't working", "stopped working",
    ],
    generic: &[
        // pronouns and determiners
        "it", "it's", "its", "this", "that", "these", "those", "them", "they", "something",
        "everything", "anything", "whatever", "all", "some", "any", "more", "the", "a", "an",
        "i", "we", "you", "me", "my", "our", "your", "there", "here", "one",
        // function words
        "to", "of", "in", "on", "for", "with", "from", "up", "out", "about", "and", "or", "but",
        "so", "is", "are", "was", "be", "been", "not", "can", "could", "would", "will",
        "please", "just", "now", "too", "very", "really", "still", "anymore", "asap", "again",
        "do", "does", "doesn't", "did", "get", "getting", "got", "take", "look", "keeps",
        "keep", "what", "why", "how", "we've", "discussed", "talked", "mentioned",
        // generic engineering nouns
        "code", "codebase", "bug", "issue", "error", "exception", "warning", "problem", "crash",
        "test", "doc", "docs", "documentation", "comment", "app", "application", "project",
        "repo", "feature", "ticket", "logging", "log", "handling", "build", "ci", "pipeline",
        "linter", "lint", "performance", "dependency", "dependencies", "version", "stuff",
        "thing", "case", "edge",
        // generic qualities
        "better", "faster", "cleaner", "robust", "properly", "correctly", "slow", "fast", "new",
        "wrong", "working", "work", "broken", "failing", "red",
    ],
    continuations: &["same", "previous", "again", "above", "before", "other", "rest", "remaining"],
    question_words: &["why", "what", "how", "which", "where", "when", "who", "is", "are", "does", "did"],
    done_markers: &[
        "should", "must", "expect", "so that", "until", "pass", "passes", "test", "tests",
        "returns", "verify", "done when", "acceptance", "without breaking",
    ],
    line_words: &["line"],
    markers: &["the", "is", "and", "to", "it", "this", "with", "for", "a", "of", "in", "on", "can", "you"],
    messages: Messages {
        vague_target: "No concrete target. Name the file, function, or exact error message so \
                       Claude doesn't spend turns searching for it.",
        large_paste: "~{n} tokens pasted. Point Claude at a file path instead, or keep only the \
                      relevant part (e.g. the first error and its stack trace).",
        no_done_criteria: "Say what \"done\" looks like (e.g. \"the tests in auth/ pass\", \
                           \"returns 404 for unknown ids\") so Claude knows when to stop.",
        held: "Prompt held by pacenote ({codes}). Suggested rewrite:",
        hold_hint: "Paste the rewrite (edit it if needed), or resubmit your original prompt unchanged to send it as-is.",
    },
};

#[rustfmt::skip]
pub static EL: Lang = Lang {
    code: "el",
    task_verbs: &[
        "διορθ*", "φτιαξ*", "φτιαχν*", "προσθεσ*", "προσθεστ*", "υλοποιησ*", "υλοποιηστ*",
        "βελτιωσ*", "βελτιωστ*", "ενημερωσ*", "ενημερωστ*", "αλλαξ*", "δημιουργησ*",
        "γραψ*", "αφαιρεσ*", "σβησ*", "διαγραψ*", "μετονομασ*", "βελτιστοποιησ*", "χειρισ*",
        "λυσ*", "επιλυσ*", "καθαρισ*", "κανε", "καν", "ερευνησ*", "ψαξ*", "επιταχυν*",
    ],
    task_phrases: &[
        "δεν δουλευει", "δεν λειτουργει", "δε δουλευει", "δε λειτουργει", "χαλασμεν*",
        "σπασμεν*", "αποτυγχαν*", "σκαει", "σκανε", "κρασαρ*", "κοκκιν*",
    ],
    generic: &[
        // articles, pronouns, determiners
        "ο", "η", "το", "οι", "τα", "του", "τησ", "των", "τον", "την", "τη", "τουσ", "τισ",
        "ενα", "ενασ", "μια", "μιασ", "ενοσ", "αυτο", "αυτα", "αυτη", "αυτοσ", "αυτον", "αυτην",
        "αυτεσ", "αυτουσ", "εκεινο", "κατι", "ολα", "ολο", "ολεσ", "ολουσ", "μερικα",
        "περισσοτερα", "περισσοτερεσ", "μου", "μασ", "σου", "σασ", "τ", "γι", "αυτ",
        // function words
        "να", "και", "σε", "στο", "στη", "στην", "στα", "στον", "στισ", "στουσ", "με", "για",
        "απο", "ειναι", "ηταν", "δεν", "δε", "μην", "θα", "πωσ", "τι", "γιατι", "ακομα", "τωρα",
        "πια", "παρακαλω", "μπορεισ", "μπορειτε", "λιγο", "οτι", "που", "εδω", "εκει", "πιο",
        "ετσι", "ωστε",
        // generic engineering nouns
        "κωδικα*", "bug", "bugs", "σφαλμα*", "λαθ*", "προβλημα*", "τεστ", "δοκιμ*", "εφαρμογ*",
        "λειτουργι*", "τεκμηριωσ*", "σχολι*", "αποδοσ*", "εκδοσ*", "εξαρτησ*", "πραγμα*",
        "περιπτωσ*", "ακραι*", "καταγραφ*", "χειρισμ*", "εξαιρεσ*", "προειδοποιησ*", "κωδικ*",
        // generic qualities
        "καλυτερ*", "γρηγορ*", "σωστ*", "αργ*", "καθαρ*", "δουλευ*", "λειτουργει",
        "χαλασμεν*", "σπασμεν*", "κοκκιν*",
    ],
    continuations: &[
        "ιδιο", "ιδια", "ιδιοσ", "ιδιεσ", "ιδιουσ", "ιδιο τροπο", "προηγουμεν*", "ξανα", "παλι",
        "αλλα", "αλλο", "αλλουσ", "αλλεσ", "αλλη", "αλλον", "αλλων", "παραπανω", "υπολοιπ*",
    ],
    question_words: &["γιατι", "τι", "πωσ", "ποιο", "ποια", "ποιοσ", "ποιεσ", "που", "ποτε", "ειναι", "μηπωσ"],
    done_markers: &[
        "πρεπει", "ωστε", "μεχρι", "περν*", "τεστ", "επιστρεφ*", "επαληθευσ*", "επιβεβαιωσ*",
    ],
    line_words: &["γραμμη"],
    markers: &[],
    messages: Messages {
        vague_target: "Δεν υπάρχει συγκεκριμένος στόχος. Ανάφερε το αρχείο, τη συνάρτηση ή το \
                       ακριβές μήνυμα σφάλματος, ώστε το Claude να μη σπαταλά γύρους ψάχνοντας.",
        large_paste: "Επικολλήθηκαν ~{n} tokens. Δώσε στο Claude τη διαδρομή του αρχείου ή κράτα \
                      μόνο το σχετικό κομμάτι (π.χ. το πρώτο σφάλμα και το stack trace του).",
        no_done_criteria: "Πες πότε θεωρείται «έτοιμο» (π.χ. «περνάνε τα τεστ στο auth/», \
                           «επιστρέφει 404 για άγνωστα id»), ώστε το Claude να ξέρει πότε να σταματήσει.",
        held: "Το prompt κρατήθηκε από το pacenote ({codes}). Προτεινόμενη αναδιατύπωση:",
        hold_hint: "Επικόλλησε την αναδιατύπωση (με αλλαγές αν χρειάζεται) ή ξαναστείλε το αρχικό prompt αυτούσιο για να σταλεί όπως είναι.",
    },
};

#[rustfmt::skip]
pub static FR: Lang = Lang {
    code: "fr",
    task_verbs: &[
        "corrig*", "repar*", "ajout*", "implement*", "refactor*", "debug*", "debogu*",
        "actualis*", "modifi*", "chang*", "fais", "faire", "rends", "rendre", "cree", "creer",
        "creez", "ecri*", "construi*", "optimis*", "amelior*", "supprim*", "enleve", "retire",
        "renomm*", "migr*", "gere", "gerer", "gerez", "resou*", "regle", "regler", "reglez",
        "nettoi*", "nettoy*", "accelere*", "mets", "mettez", "mettre", "passe", "permets",
    ],
    task_phrases: &[
        "ne marche pas", "marche pas", "ne fonctionne pas", "fonctionne pas", "marche plus",
        "fonctionne plus", "est casse", "casse", "plante", "plantent", "echoue", "echouent",
        "est rouge", "en rouge",
    ],
    generic: &[
        // articles, pronouns, determiners
        "le", "la", "les", "l", "un", "une", "des", "du", "de", "d", "ce", "cet", "cette", "ces",
        "ca", "cela", "ceci", "il", "elle", "ils", "on", "je", "j", "tu", "vous", "nous", "me",
        "m", "te", "t", "moi", "mon", "ma", "mes", "notre", "nos", "ton", "ta", "votre", "tout",
        "tous", "toute", "toutes", "quelque", "chose", "quelques", "truc", "trucs", "plus", "en",
        "y",
        // function words
        "au", "aux", "a", "dans", "sur", "pour", "avec", "par", "et", "ou", "mais", "est", "sont",
        "etait", "pas", "ne", "n", "s", "plait", "stp", "svp", "peux", "pouvez", "merci",
        "encore", "maintenant", "juste", "tres", "vite", "qu", "que", "quoi", "pourquoi",
        "comment", "sorte", "faire", "fais", "qui", "jour",
        // generic engineering nouns
        "code", "bug", "bugs", "bogue*", "probleme*", "erreur*", "exception*", "avertissement*",
        "warning*", "crash", "plantage", "test", "tests", "doc", "docs", "documentation",
        "commentaire*", "appli", "application", "app", "projet", "depot", "fonctionnalite*",
        "ticket", "log", "logs", "journalisation", "gestion", "build", "ci", "pipeline",
        "linter", "performance*", "perf", "dependance*", "version", "cas", "limite*",
        // generic qualities
        "rapide*", "propre", "robuste", "mieux", "meilleur*", "correctement", "lent*", "nouveau",
        "nouvelle", "marche", "fonctionne", "casse", "rouge",
    ],
    continuations: &[
        "meme", "precedent*", "encore", "autre", "autres", "ci-dessus", "dessus", "avant", "reste",
    ],
    question_words: &["pourquoi", "quoi", "comment", "quel", "quelle", "quels", "quelles", "ou", "quand", "qui", "que", "est-ce", "est"],
    done_markers: &[
        "doit", "doivent", "devrait", "pour que", "afin que", "jusqu'a", "passe", "passent",
        "test", "tests", "renvoie", "renvoyer", "retourne", "verifie*",
    ],
    line_words: &["ligne"],
    markers: &[
        "le", "la", "les", "des", "est", "une", "pas", "ne", "ce", "et", "pour", "dans", "avec",
        "ca", "l", "d", "du", "tu", "je", "que",
    ],
    messages: Messages {
        vague_target: "Aucune cible précise. Indiquez le fichier, la fonction ou le message \
                       d'erreur exact pour que Claude ne perde pas de tours à chercher.",
        large_paste: "~{n} tokens collés. Indiquez plutôt le chemin du fichier à Claude, ou ne \
                      gardez que la partie utile (par ex. la première erreur et sa stack trace).",
        no_done_criteria: "Dites à quoi ressemble « terminé » (par ex. « les tests de auth/ \
                           passent », « renvoie 404 pour un id inconnu ») pour que Claude sache \
                           quand s'arrêter.",
        held: "Prompt retenu par pacenote ({codes}). Réécriture suggérée :",
        hold_hint: "Collez la réécriture (modifiez-la si besoin), ou renvoyez votre prompt d'origine tel quel pour l'envoyer sans changement.",
    },
};

#[rustfmt::skip]
pub static ES: Lang = Lang {
    code: "es",
    task_verbs: &[
        "arregl*", "corrig*", "repar*", "anad*", "agreg*", "implement*", "refactoriz*",
        "depur*", "debug*", "actualiz*", "cambi*", "haz", "hazlo", "hacer", "crea", "crear",
        "escrib*", "constru*", "optimiz*", "mejora", "mejorar", "mejoralo", "elimin*", "borr*",
        "quit*", "renombr*", "migr*", "manej*", "resuelv*", "resolv*", "soluciona*", "limpi*",
        "aceler*", "sube", "permite",
    ],
    task_phrases: &[
        "no funciona", "no anda", "esta roto", "esta rota", "falla", "fallan", "se cae",
        "se rompe", "crashea", "en rojo", "esta rojo",
    ],
    generic: &[
        // articles, pronouns, determiners
        "el", "la", "los", "las", "lo", "le", "un", "una", "unos", "unas", "esto", "esta", "este",
        "eso", "esa", "ese", "estos", "estas", "algo", "todo", "todos", "todas", "cosa", "cosas",
        "mas", "me", "mi", "mis", "nuestro", "tu", "su", "se", "te",
        // function words
        "al", "del", "de", "a", "en", "con", "para", "por", "y", "o", "pero", "es", "son", "era",
        "esta", "no", "que", "porfa", "favor", "puedes", "podes", "podrias", "ahora", "solo",
        "muy", "ya", "aqui", "alli", "como", "otra", "vez", "funcione", "cae", "caen", "rompe",
        // generic engineering nouns
        "codigo", "bug", "bugs", "error", "errores", "problema*", "excepcion*", "advertencia*",
        "warning*", "fallo*", "crash", "prueba*", "test", "tests", "doc", "docs",
        "documentacion", "comentario*", "app", "aplicacion", "proyecto", "repo",
        "funcionalidad*", "feature", "ticket", "log", "logs", "logging", "registro*", "manejo",
        "build", "ci", "pipeline", "linter", "rendimiento", "dependencia*", "version", "caso*",
        "borde*", "limite*",
        // generic qualities
        "mejor", "rapido*", "limpio", "robusto", "bien", "correctamente", "lento", "nuevo",
        "nueva", "funciona", "roto", "rota", "rojo",
    ],
    continuations: &[
        "mismo", "misma", "igual", "anterior*", "otra vez", "otro", "otros", "otras", "de nuevo",
        "arriba", "antes", "resto",
    ],
    question_words: &["que", "como", "cual", "cuales", "donde", "cuando", "quien", "por", "es", "esta"],
    done_markers: &[
        "debe", "deben", "deberia", "para que", "hasta que", "pase", "pasen", "test", "tests",
        "prueba", "pruebas", "devuelve", "devolver", "retorna", "verific*",
    ],
    line_words: &["linea"],
    markers: &[
        "el", "los", "las", "una", "es", "que", "para", "con", "por", "esta", "y", "esto", "lo",
        "del", "se", "eso",
    ],
    messages: Messages {
        vague_target: "No hay un objetivo concreto. Indica el archivo, la función o el mensaje \
                       de error exacto para que Claude no gaste turnos buscándolo.",
        large_paste: "~{n} tokens pegados. Indica a Claude la ruta del archivo o deja solo la \
                      parte relevante (p. ej. el primer error y su stack trace).",
        no_done_criteria: "Di cuándo está «terminado» (p. ej. «pasan los tests de auth/», \
                           «devuelve 404 para ids desconocidos») para que Claude sepa cuándo parar.",
        held: "pacenote retuvo el prompt ({codes}). Reescritura sugerida:",
        hold_hint: "Pega la reescritura (edítala si hace falta) o reenvía tu prompt original sin cambios para enviarlo tal cual.",
    },
};

#[rustfmt::skip]
pub static DE: Lang = Lang {
    code: "de",
    task_verbs: &[
        "behebe", "beheb*", "fix*", "reparier*", "korrigier*", "fueg*", "hinzufueg*",
        "implementier*", "refaktorier*", "debugg*", "aktualisier*", "aender*", "mach", "mache",
        "machen", "erstell*", "schreib*", "bau", "baue", "optimier*", "verbesser*", "entfern*",
        "loesch*", "umbenenn*", "benenn*", "migrier*", "behandl*", "loes", "loese",
        "raeum*", "bereinig*", "beschleunig*", "sorg", "sorge", "stell",
    ],
    task_phrases: &[
        "funktioniert nicht", "geht nicht", "laeuft nicht", "kaputt", "stuerzt ab",
        "schlaegt fehl", "schlagen fehl", "fehlschlagend*", "ist rot", "geht nicht mehr",
    ],
    generic: &[
        // articles, pronouns, determiners
        "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "es",
        "dies", "diese", "dieser", "dieses", "etwas", "alles", "alle", "ich", "wir", "du", "sie",
        "mir", "mich", "mein", "meine", "unser", "unsere", "dein", "dafuer", "dass", "mehr",
        // function words
        "bitte", "mal", "jetzt", "noch", "nur", "sehr", "und", "oder", "aber", "ist", "sind",
        "war", "nicht", "in", "im", "auf", "fuer", "mit", "von", "vom", "zu", "zum", "zur",
        "kannst", "koenntest", "kann", "hier", "da", "dort", "ab", "doch", "hinzu", "auf",
        "heile", "dafuer", "stuerzt",
        // generic engineering nouns
        "code", "bug", "bugs", "fehler", "problem", "probleme", "ausnahme*", "warnung*",
        "absturz", "test", "tests", "doku", "dokumentation", "kommentar*", "app", "anwendung",
        "projekt", "repo", "feature", "funktion*", "ticket", "log", "logs", "logging",
        "fehlerbehandlung", "behandlung", "build", "ci", "pipeline", "linter", "performance",
        "leistung", "abhaengigkeit*", "version", "sache*", "ding*", "fall", "faelle",
        "randfall*", "randfaelle",
        // generic qualities
        "besser", "schneller", "sauber*", "robust*", "richtig", "korrekt", "langsam", "neu",
        "neue", "neuen", "neuer", "funktioniert", "geht", "kaputt", "rot",
        "fehlschlagend*",
    ],
    continuations: &[
        "gleich*", "genauso", "vorherig*", "wieder", "nochmal", "andere", "anderen", "anderer",
        "anderes", "oben", "vorhin", "rest", "dasselbe", "denselben", "dieselbe",
    ],
    question_words: &["warum", "was", "wie", "welche", "welcher", "welches", "wo", "wann", "wer", "ist", "sind", "gibt"],
    done_markers: &[
        "soll", "sollte", "muss", "muessen", "damit", "bis", "besteh*", "durchlaeuft",
        "durchlaufen", "gruen", "test", "tests", "zurueck*", "liefert", "pruef*", "verifizier*",
    ],
    line_words: &["zeile"],
    markers: &[
        "der", "die", "das", "und", "ist", "nicht", "ein", "eine", "mit", "bitte", "den", "es",
        "du", "dem", "zu", "auf", "fuer",
    ],
    messages: Messages {
        vague_target: "Kein konkretes Ziel. Nenne die Datei, die Funktion oder die genaue \
                       Fehlermeldung, damit Claude keine Runden mit Suchen verschwendet.",
        large_paste: "~{n} Tokens eingefügt. Gib Claude stattdessen den Dateipfad oder behalte \
                      nur den relevanten Teil (z. B. den ersten Fehler und seinen Stacktrace).",
        no_done_criteria: "Beschreibe, wann es „fertig“ ist (z. B. „die Tests in auth/ laufen \
                           durch“, „liefert 404 für unbekannte IDs“), damit Claude weiß, wann es \
                           aufhören kann.",
        held: "Prompt von pacenote zurückgehalten ({codes}). Vorgeschlagene Umformulierung:",
        hold_hint: "Füge die Umformulierung ein (bei Bedarf angepasst) oder sende deinen ursprünglichen Prompt unverändert erneut, um ihn so zu schicken.",
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        fold(text).split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn folds_accents_umlauts_and_final_sigma() {
        assert_eq!(fold("Διόρθωσέ το λάθος"), "διορθωσε το λαθοσ");
        assert_eq!(fold("Ändere die Größe"), "aendere die groesse");
        assert_eq!(fold("Arréglalo, añade más"), "arreglalo, anade mas");
        assert_eq!(fold("l’appli plante"), "l'appli plante");
    }

    #[test]
    fn prefix_entries_match_inflections() {
        assert!(entry_matches("διορθ*", "διορθωστε"));
        assert!(entry_matches("corrig*", "corrigez"));
        assert!(!entry_matches("fix", "fixed"));
    }

    #[test]
    fn umlaut_transliteration_keeps_aendern_and_andere_apart() {
        assert!(list_has(DE.task_verbs, &fold("ändere")));
        assert!(!list_has(DE.task_verbs, &fold("andere")));
    }

    #[test]
    fn detects_languages() {
        for (text, code) in [
            ("διόρθωσε το bug", "el"),
            ("corrige le bug dans le code", "fr"),
            ("arregla el bug en el código", "es"),
            ("behebe den Bug in der App", "de"),
            ("fix the bug in the app", "en"),
            ("fix it", "en"),
            ("arréglalo", "es"),
            ("füge Logging hinzu", "de"),
        ] {
            assert_eq!(detect(&words(text), text)[0].code, code, "{text}");
        }
    }

    #[test]
    fn evidenced_languages_stay_active_alongside_english() {
        let codes: Vec<_> = detect(&words("fix el bug de la app"), "fix el bug de la app")
            .iter()
            .map(|l| l.code)
            .collect();
        assert_eq!(codes[0], "en");
        assert!(codes.contains(&"fr") && codes.contains(&"es"));
    }

    #[test]
    fn every_pack_is_folded() {
        for lang in ALL {
            let lists = [
                lang.task_verbs,
                lang.task_phrases,
                lang.generic,
                lang.continuations,
                lang.question_words,
                lang.done_markers,
                lang.line_words,
                lang.markers,
            ];
            for entry in lists.iter().flat_map(|l| l.iter()) {
                let bare = entry.trim_end_matches('*');
                assert_eq!(
                    fold(bare),
                    bare,
                    "{}: entry {entry:?} is not folded",
                    lang.code
                );
            }
        }
    }
}
