// lsdoc -- the descriptions of the built-ins and of the standard library for the language
// services (ls.rs): parameters and a short German text per name, in our own words
//
//   DOCS                 (name, receiver, parameters, description): "str.split" with the
//                        receiver "s" is the method s.split(sep, n); an empty receiver is a
//                        function of the module (str.from_code, math.sqrt) or a constant
//                        (math.pi, parameters None); (G9.2) the descriptions are keys of
//                        msg.rs (de!), Doc::doc() gives them in the current language
//   doc(name)            the entry of a name ("print", "str.split", "math.pi")
//   methods(ty)          the methods of a type ("str", "list", "map", "bytes")
//   MODULES              the names that hold functions (str, list, map, bytes, math, json,
//                        int, float)
//
// scripttest (group scriptls) checks that every name of script::library() has an entry.

/// a name of the library: name, the receiver of a method ("" for module functions and
/// constants), the parameters (None: a constant), a short description
pub(crate) struct Doc {
    pub name: &'static str,
    pub recv: &'static str,
    pub params: Option<&'static str>,
    pub text: &'static str,
}

impl Doc {
    /// (G9.2) the description in the current language (msg.rs; the texts are its keys)
    pub fn doc(&self) -> &'static str {
        crate::msg::tr(self.text)
    }
}

const fn d(name: &'static str, recv: &'static str, params: &'static str, text: &'static str) -> Doc {
    Doc { name, recv, params: Some(params), text }
}

const fn c(name: &'static str, text: &'static str) -> Doc {
    Doc { name, recv: "", params: None, text }
}

/// the modules of the library (their functions are module.name)
pub(crate) const MODULES: &[&str] = &["str", "list", "map", "bytes", "math", "json", "int", "float"];

/// see the module comment
pub(crate) const DOCS: &[Doc] = &[
    d("print", "", "x, ..", de!("Schreibt die Werte mit Leerzeichen dazwischen als eine Zeile in die Ausgabe.")),
    d("len", "", "x", de!("Die Länge: Zeichen eines str, Bytes von bytes, Einträge einer list oder map.")),
    d("type", "", "x", de!("Der Typ als Text: nil, bool, int, float, str, bytes, list, map oder fn.")),
    d("str", "", "x", de!("Der Wert als Text, wie print ihn schreibt.")),
    d("int", "", "x", de!("Eine ganze Zahl: schneidet einen float ab, liest Text wie \"42\" oder \"0x1F\".")),
    d("float", "", "x", de!("Der Wert als Gleitkommazahl (auch aus Text).")),
    d("bytes", "", "x, ..", de!("Neue Bytes: leer, aus Text (UTF-8), aus einer Liste von Zahlen oder aus einzelnen Zahlen.")),
    // modules and tests (module.rs, stdlib.rs)
    d("import", "", "path", de!("Lädt eine andere Datei einmal und gibt ihre obersten fns und lets als Map; Namen mit _ bleiben privat.")),
    d("assert", "", "cond, msg", de!("Wirft einen Fehler mit msg, wenn cond nicht wahr ist.")),
    d("assert_eq", "", "a, b, msg", de!("Wirft einen Fehler, wenn a und b verschieden sind; Listen und Maps zählen nach ihrem Inhalt.")),
    // str
    d("str.len", "s", "", de!("Die Zahl der Zeichen.")),
    d("str.slice", "s", "start, end", de!("Der Teil von start bis vor end (Zeichen); negative Werte zählen vom Ende.")),
    d("str.split", "s", "sep, n", de!("Zerlegt den Text an sep in eine Liste, höchstens n Teile; ohne sep an Leerraum.")),
    d("str.join", "sep", "list", de!("Die Werte der Liste als ein Text, mit sep dazwischen.")),
    d("str.trim", "s", "chars", de!("Ohne Leerraum an beiden Enden (oder ohne die Zeichen aus chars).")),
    d("str.trim_start", "s", "chars", de!("Ohne Leerraum am Anfang (oder ohne die Zeichen aus chars).")),
    d("str.trim_end", "s", "chars", de!("Ohne Leerraum am Ende (oder ohne die Zeichen aus chars).")),
    d("str.find", "s", "sub, from", de!("Wo sub zuerst vorkommt (ab Zeichen from), nil wenn nirgends.")),
    d("str.find_last", "s", "sub", de!("Wo sub zuletzt vorkommt, nil wenn nirgends.")),
    d("str.contains", "s", "sub", de!("Ob sub im Text vorkommt.")),
    d("str.starts_with", "s", "p", de!("Ob der Text mit p anfängt.")),
    d("str.ends_with", "s", "p", de!("Ob der Text mit p aufhört.")),
    d("str.count", "s", "sub", de!("Wie oft sub vorkommt, ohne Überlappung.")),
    d("str.replace", "s", "from, to, n", de!("Setzt to für from ein, überall oder nur n Mal.")),
    d("str.upper", "s", "", de!("Der Text in Großbuchstaben (ß wird SS).")),
    d("str.lower", "s", "", de!("Der Text in Kleinbuchstaben.")),
    d("str.repeat", "s", "n", de!("Der Text n Mal hintereinander.")),
    d("str.chars", "s", "", de!("Die Zeichen als Liste von Texten.")),
    d("str.bytes", "s", "", de!("Der Text als bytes in UTF-8.")),
    d("str.code", "s", "i", de!("Der Unicode-Wert des Zeichens an Stelle i (ohne i: des ersten).")),
    d("str.from_code", "", "n, ..", de!("Ein Text aus den Zeichen mit diesen Unicode-Werten.")),
    d("str.format", "s", "args, ..", de!("Setzt die Argumente für {}, {0} oder {name} ein; nach einem Doppelpunkt Breite, Ausrichtung und Genauigkeit.")),
    d("str.hash", "s", "", de!("Ein fester 32-Bit-Hash (FNV-1a) der UTF-8-Bytes, gleich auf jedem Rechner.")),
    // list
    d("list.len", "l", "", de!("Die Zahl der Werte.")),
    d("list.push", "l", "x, ..", de!("Hängt die Werte hinten an.")),
    d("list.pop", "l", "", de!("Nimmt den letzten Wert heraus und gibt ihn zurück (nil bei leerer Liste).")),
    d("list.insert", "l", "i, x", de!("Fügt x an Stelle i ein (0 bis len).")),
    d("list.remove", "l", "i", de!("Nimmt den Wert an Stelle i heraus und gibt ihn zurück.")),
    d("list.slice", "l", "start, end", de!("Eine neue Liste von start bis vor end; negative Werte zählen vom Ende.")),
    d("list.reverse", "l", "", de!("Dreht die Reihenfolge um (in der Liste selbst) und gibt sie zurück.")),
    d("list.sort", "l", "cmp", de!("Sortiert stabil (in der Liste selbst); cmp(a, b) gibt eine Zahl oder true für a vor b.")),
    d("list.map", "l", "f", de!("Eine neue Liste mit f(x) für jeden Wert.")),
    d("list.filter", "l", "f", de!("Eine neue Liste mit den Werten, für die f(x) wahr ist.")),
    d("list.reduce", "l", "f, init", de!("Faltet die Liste mit f(acc, x) zusammen, ab init (ohne init: ab dem ersten Wert).")),
    d("list.contains", "l", "x", de!("Ob x in der Liste steht (Vergleich mit ==).")),
    d("list.index", "l", "x", de!("Die erste Stelle von x, nil wenn es fehlt.")),
    d("list.join", "l", "sep", de!("Die Werte als ein Text, mit sep dazwischen.")),
    d("list.range", "", "start, end, step", de!("Eine neue Liste start, start + step, … bis vor end (step 1, auch negativ); list.range(n) zählt von 0 bis n - 1. Ganze Zahlen bleiben ganz.")),
    d("list.linspace", "", "start, end, n", de!("Eine neue Liste von n Kommazahlen, gleichmäßig verteilt von start bis end (beide dabei).")),
    // map
    d("map.len", "m", "", de!("Die Zahl der Einträge.")),
    d("map.keys", "m", "", de!("Die Schlüssel als Liste, in der Reihenfolge des Einfügens.")),
    d("map.values", "m", "", de!("Die Werte als Liste, in der Reihenfolge des Einfügens.")),
    d("map.entries", "m", "", de!("Die Einträge als Liste von [Schlüssel, Wert].")),
    d("map.has", "m", "k", de!("Ob es den Schlüssel k gibt.")),
    d("map.get", "m", "k, default", de!("Der Wert zu k, default wenn der Schlüssel fehlt.")),
    d("map.delete", "m", "k", de!("Entfernt den Eintrag zu k; ob es ihn gab.")),
    d("map.merge", "m", "other, ..", de!("Eine neue Map mit den Einträgen aller Maps; spätere Werte gewinnen.")),
    // bytes
    d("bytes.len", "b", "", de!("Die Zahl der Bytes.")),
    d("bytes.slice", "b", "start, end", de!("Die Bytes von start bis vor end; negative Werte zählen vom Ende.")),
    d("bytes.hex", "b", "", de!("Die Bytes als Hex-Text.")),
    d("bytes.from_hex", "", "hex", de!("Bytes aus einem Hex-Text.")),
    d("bytes.text", "b", "lossy", de!("Die Bytes als Text (UTF-8); lossy ersetzt ungültige Folgen statt zu werfen.")),
    d("bytes.hash", "b", "", de!("Ein fester 32-Bit-Hash (FNV-1a) der Bytes.")),
    // math
    d("math.abs", "", "x", de!("Der Betrag (ein int bleibt ein int).")),
    d("math.min", "", "a, ..", de!("Der kleinste der Werte (oder einer Liste).")),
    d("math.max", "", "a, ..", de!("Der größte der Werte (oder einer Liste).")),
    d("math.clamp", "", "x, lo, hi", de!("x begrenzt auf lo bis hi.")),
    d("math.floor", "", "x", de!("Abgerundet auf eine ganze Zahl.")),
    d("math.ceil", "", "x", de!("Aufgerundet auf eine ganze Zahl.")),
    d("math.round", "", "x", de!("Gerundet, Hälften weg von der Null.")),
    d("math.trunc", "", "x", de!("Die Nachkommastellen abgeschnitten.")),
    d("math.sqrt", "", "x", de!("Die Quadratwurzel.")),
    d("math.pow", "", "x, y", de!("x hoch y.")),
    d("math.exp", "", "x", de!("e hoch x.")),
    d("math.log", "", "x", de!("Der natürliche Logarithmus.")),
    d("math.log2", "", "x", de!("Der Logarithmus zur Basis 2.")),
    d("math.log10", "", "x", de!("Der Logarithmus zur Basis 10.")),
    d("math.sin", "", "x", de!("Der Sinus (x im Bogenmaß).")),
    d("math.cos", "", "x", de!("Der Kosinus (x im Bogenmaß).")),
    d("math.tan", "", "x", de!("Der Tangens (x im Bogenmaß).")),
    d("math.asin", "", "x", de!("Der Arkussinus im Bogenmaß, von -π/2 bis π/2; nan außerhalb von -1 bis 1.")),
    d("math.acos", "", "x", de!("Der Arkuskosinus im Bogenmaß, von 0 bis π; nan außerhalb von -1 bis 1.")),
    d("math.atan", "", "x", de!("Der Arkustangens im Bogenmaß, von -π/2 bis π/2.")),
    d("math.atan2", "", "y, x", de!("Der Winkel des Punkts (x, y) im Bogenmaß, von -π bis π: atan(y / x) im richtigen Quadranten.")),
    d("math.hypot", "", "x, y, ..", de!("Die Länge des Vektors (auch einer Liste): die Wurzel aus der Summe der Quadrate, genau gerundet, ohne Überlauf.")),
    d("math.sin_deg", "", "x", de!("Der Sinus von x Grad; exakt bei Vielfachen von 30° (0, ±0.5, ±1).")),
    d("math.cos_deg", "", "x", de!("Der Kosinus von x Grad; exakt bei Vielfachen von 30° (0, ±0.5, ±1).")),
    d("math.tan_deg", "", "x", de!("Der Tangens von x Grad; exakt bei Vielfachen von 45° (0, ±1, ±inf bei 90°).")),
    d("math.is_nan", "", "x", de!("Ob x keine Zahl (nan) ist.")),
    d("math.is_finite", "", "x", de!("Ob x endlich ist (weder inf noch nan).")),
    c("math.pi", de!("Die Kreiszahl π.")),
    c("math.e", de!("Die Eulersche Zahl e.")),
    c("math.tau", de!("2π, ein voller Kreis im Bogenmaß.")),
    c("math.inf", de!("Unendlich als float.")),
    c("math.nan", de!("Keine Zahl (nan) als float.")),
    // json, int, float
    d("json.encode", "", "v, indent", de!("Der Wert als JSON-Text: kompakt oder mit indent Leerzeichen je Ebene.")),
    d("json.decode", "", "text", de!("Die Werte aus einem JSON-Text (Objekte werden Maps in ihrer Reihenfolge).")),
    d("int.parse", "", "s, base", de!("Die ganze Zahl in s (auch 0x, 0b, 0o), nil wenn s keine ist.")),
    c("int.max", de!("Die größte ganze Zahl.")),
    c("int.min", de!("Die kleinste ganze Zahl.")),
    d("float.parse", "", "s", de!("Die Gleitkommazahl in s, nil wenn s keine ist.")),
];

/// the entry of a name
pub(crate) fn doc(name: &str) -> Option<&'static Doc> {
    DOCS.iter().find(|x| x.name == name)
}

/// (S3) the parameters of a built-in or library function as a signature, the receiver of a
/// method first ("str.split": s, sep, n; "math.min": a and any number more), for named
/// arguments; None for a constant or an unknown name
pub(crate) fn sig(name: &str) -> Option<crate::api::Sig> {
    let d = doc(name)?;
    let p = d.params?;
    let all = match (d.recv.is_empty(), p.is_empty()) {
        (true, _) => alloc::string::String::from(p),
        (false, true) => alloc::string::String::from(d.recv),
        (false, false) => alloc::format!("{}, {}", d.recv, p),
    };
    Some(crate::api::Sig { params: crate::api::parse_params(&all), ret: alloc::string::String::new() })
}

/// the methods of type ty (entries with a receiver)
pub(crate) fn methods(ty: &str) -> impl Iterator<Item = &'static Doc> + '_ {
    DOCS.iter().filter(move |x| !x.recv.is_empty() && x.name.split_once('.').is_some_and(|(t, _)| t == ty))
}
