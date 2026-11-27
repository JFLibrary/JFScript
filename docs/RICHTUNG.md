# JFScript: Richtung (Entscheidungspapier)

Stand 2026-11-23, entschieden am selben Tag. Das Papier soll festlegen, wofür JFScript da ist und wie es wächst, bevor
eine einzelne App es in ihre Richtung ausbaut. Bis alle Grundfragen entschieden sind, bleibt JFScript unverändert;
`modeltool`, JFNetworker und die Tests kommen mit der heutigen Sprache aus.

## Entscheidungen

| Nr. | Frage | Entscheidung des Nutzers | Datum |
| --- | --- | --- | --- |
| G1 | Eine Sprache für alle JF-Apps? | **a mit d:** JFScript wird die Skriptsprache aller JF-Apps (Simulation, Modelle, Automatisierung, Erweiterungen); neue Syntax folgt JavaScript, wo es keinen Grund gibt abzuweichen. WebAssembly für Erweiterungen (c) bleibt für später offen; JavaScript über `narsil-jsvm` (b) nicht. **d lockerer gefasst (der Nutzer, 2026-11-27):** neue Syntax nimmt bekannte Sprachen zum Vorbild (JavaScript, Python, Lua), je Fall das, was zu JFScript passt und was Menschen und KIs schon kennen; JavaScript ist kein Muss. „JS/Lua-artig“ stand seit SK1 als Beschreibung in JFLibs ROADMAP; die strengere Fassung „folgt JavaScript“ kam am 2026-11-23 als Vorschlag von Claude. | 2026-11-23, 2026-11-27 |
| G2 | Determinismus, Gleitkomma | **b:** Die Sprache bleibt deterministisch (Uhr, Zufall, Dateien nur über den Host). Gleitkomma in Hardware für `+ − × ÷` und die Wurzel mit NaN in einer Form wird geprüft und nur eingebaut, wenn Tests dieselben Bits auf allen Zielen zeigen (mit der Stufe für Tempo); keine zweite Betriebsart. | 2026-11-23 |

| G3 | Typen | **Wie empfohlen:** dynamisch bleiben; Typen zuerst in den Beschreibungen des Hosts (mit G4), damit die Sprachdienste das Objektmodell einer App kennen; optionale Angaben im Skript in der Form von TypeScript als spätere Stufe, zur Laufzeit ohne Wirkung. | 2026-11-23 |

| G4 | Objekte, Eigenschaften, Operatoren, Ereignisse | **Wie empfohlen:** Host-Objekte (G4.1), Eigenschaften lesen und setzen über den Host (G4.2), Operatoren nur für Host-Typen (G4.3) und Ereignisse mit `obj.on` / `obj.off` (G4.5) als erste große Stufe (S2); Maps bleiben vorerst, später `class` nach G1 d (G4.4); Rückrufe jetzt, `await` als Kandidat für die Syntax-Stufe (G4.6). | 2026-11-23 |

| G5 | Sicherheit und Rechte | **Wie empfohlen:** Rechte je Skript in einem eigenen Rahmen über `plat::policy`, die `PERMISSIONS` der App als Grenze (G5.1); Skripte in Dokumenten beim Öffnen aus, nach Zustimmung („Einmal ausführen“, „Diesem Dokument vertrauen“ über den Hash der Skripte) und dann im engsten Rahmen, keine vertrauenswürdigen Ordner (G5.2); Erweiterungen mit einem Manifest ihrer Rechte (G5.3); Signaturen später (G5.4); Grenzen je Aufruf und ein Budget je Bild, damit die App bedienbar bleibt (G5.5). | 2026-11-23 |

| G6 | Tempo | **Wie empfohlen:** erst messen, mit den Zielwerten als Prüfungen (10 000 Shapes mit einer Eigenschaft unter 0,5 s auf der Iris Xe-Klasse, der Skript-Teil eines Modells mit 1000 Teilen unter 1 s, ein Handler unter 5 ms, JFNetworker unverändert); erst bei verfehlten Zielen optimieren; kein JIT. | 2026-11-23 |

| G7 | Verträglichkeit, Versionen, Ort | **Wie empfohlen:** Brüche nur mit einer Version der Sprache und einer automatischen Umstellung, so selten wie möglich (G7.1); `int / int` schneidet weiter ab, die Sprachdienste warnen (G7.2); Name und `.jfs` bleiben (G7.3); JFScript bleibt im eigenen Repository per git, JFLib hängt nie von JFScript ab, crates.io wird nach S2 oder für Dritte neu entschieden (G7.4; mit G8 am 2026-11-23 geschärft: JFLib bekommt keine Bausteine nur für Skripte). **G7.1 ergänzt (der Nutzer, 2026-11-27):** Solange JFScript im Teststadium ist, darf es brechen; wer bricht, stellt die vorhandenen Skripte (Tests, JFNetworker, `modeltool`) gleich mit um. Versionen der Sprache und die automatische Umstellung kommen erst mit der Freigabe für fremde Anwender. | 2026-11-23, 2026-11-27 |

| G8 | Werkzeuge für Anwender | **Getrennt von JFLib** (der Nutzer, nach einer ersten Fassung mit Bausteinen in JFLib): Alles für Skripte liegt im Repository von JFScript, in einem eigenen Crate neben der Sprache (Arbeitsname `jfscript-ide`): Debugger-Ansicht, Konsole, das Fenster „Skripte“, der Objektkatalog, die Anbindung an `plat::policy`; er baut auf den allgemeinen Widgets von JFLib (`ui.code_editor` mit seiner Schnittstelle für Sprachdienste, Bäume, Listen, Text) und nimmt `jflib` von crates.io; Apps nehmen ihn per git. In JFLib kommt nichts nur für Skripte; fehlt ein allgemeines Widget, kommt es als gewöhnliche Funktion von `lib/ui` (G8.1). Das Fenster „Skripte“ mit JFDiagrammer als erster App (G8.2); der Objektkatalog aus den Beschreibungen des Hosts (G8.3); Makro-Rekorder später je App, die Objektmodelle von Anfang an so, dass jeder Befehl einen Aufruf hat (G8.4); JFNetworker kann wechseln, muss nicht (G8.5). | 2026-11-23 |

| G9 | Sprache der Meldungen und Namen | **Wie empfohlen:** Meldungen in der Sprache der App über eine eigene Tabelle von JFScript (zuerst `de` und `en`, ohne Angabe Deutsch wie heute) (G9.1); die Texte der Sprachdienste und des Objektkatalogs auf demselben Weg, die Beschreibungen der Hosts in ihren Sprachen (G9.2); Namen in den Objektmodellen englisch, auch die Parameter, die von `modeltool` werden vor den benannten Argumenten umgestellt; Schlüsselwörter nie übersetzt (G9.3). | 2026-11-23 |

| Reihenfolge | S1 sofort oder mit S2? | **S1 sofort** (klein, ändert nichts Bestehendes, hilft `modeltool` gleich). | 2026-11-23 |

| S3 | Die Formen der neuen Syntax | **Bedingungsausdruck** `c ? a : b`; **benannte Argumente** `f(x, width: 2)` (Skriptfunktionen nach ihren Parameternamen, Host-Funktionen nach der Signatur in `script::Api`); **Zerlegen** `let [x, y] = p` und `let {a, b} = m` (auch verschachtelt, in Parametern und in `for`); **Listen aus einer Schleife** `[x * 2 for x in xs if x > 0]` wie in Python; dazu ohne eigene Frage Vorgabewerte `fn f(a, b = 2)` und die Kurzform `{a, b}`; **`await` schon in S3**, als gewöhnliches Schlüsselwort (kein Skript nutzt `await` als Namen); **keine Versionen der Sprache in S3** (G7.1 ergänzt). | 2026-11-27 |

Alle Grundfragen sind entschieden.

## Warum jetzt

Drei Stränge ziehen an JFScript, jeder mit anderen Bedürfnissen:

- **JFNetworker** nutzt es seit Oktober 2026 für die Dienste der virtuellen Hosts und für Skripte in den Seiten seines
  Webbrowsers.
- **Die Spiel-Engine** beschreibt Modelle und Karten in JFScript (`apps/modeltool`, GAME.md Entscheidung E4: „JFScript
  wird dafür ausgebaut“). Der Bewertungslauf von G7g Teil 2 hat eine Liste von Wünschen ergeben (JFLib, GEOM.md
  „Was JFScript braucht“).
- **JFDiagrammer** soll später Automatisierung und Erweiterungen über JFScript bekommen, als Ersatz für VBA und COM-Add-ins,
  mit einem Ereignismodell (FEATURES.md UI-21 und RX-10, Entscheidung F7: „später, nicht in Phase 2“).

Dazu die Richtung des Nutzers: JFScript als die Skriptsprache aller JF-Apps. Einzelne Wünsche wie „Vektoren mit
Operatoren“ würden diese Richtung nebenbei festlegen (Operatoren für wen? Host-Objekte wie?). Deshalb erst die
Grundfragen, dann die Stufen.

## Was JFScript heute ist

**Die Sprache.** Klein und dynamisch typisiert, mit einer Syntax nahe an JavaScript: `let`, `fn`, Closures,
geschweifte Klammern, `if`/`while`/`for x in`, Bereiche `0..n`, `try`/`catch`, `??`, Text mit `${..}`, Module über die
Funktion `import(path)`. Werte: `nil`, `bool`, `int` (i64), `float`, `str`, `bytes`, `list`, `map`, `fn`. Objekte sind
Maps; eigene Typen, Klassen und Operatoren für eigene Typen gibt es nicht. Fehlermeldungen sind deutsch.

**Die VM.** Bytecode, ein eigener Heap mit Sammler, harte Grenzen für Speicher, Stapel, Tiefe und Ausgabe, Brennstoff
(Rechenzeit) je Aufruf; ein Lauf lässt sich an jeder Stelle anhalten und fortsetzen (`Status::Yielded`), eine VM lässt
sich mitten im Lauf kopieren (`Vm: Clone + Send`).

**Deterministisch.** Keine Uhr, kein Zufall, keine Dateien in der Sprache selbst; Gleitkomma rechnet `script::num` in
Software, die Mathematik ist fdlibm darauf. So gibt ein Skript auf jedem Ziel dieselben Bits. Der Grund für die Software
steht in `num.rs`: NaN-Bits und Modi wie Flush-to-Zero der Prozessoren; für endliche Werte würden x86-64, ARM64 und wasm
in der Hardware übereinstimmen. Uhr, Zufall und Dateien gibt es nur, wenn der Host sie als Funktionen anbietet (der
Runner `jfs` tut das mit `now`, `rand`, `timer`, `disk`).

**Die Schnittstelle zum Host.** Der Host beantwortet Namen: `f(..)` und `ns.f(..)` rufen `Host::call("f" | "ns.f")`,
`obj.m(..)` auf einem Wert ohne eigene Methode `Host::call("list.m", [obj, ..])`. Werte, die der Host behält, werden
angeheftet (`pin`). Es gibt keine eigenen Werte des Hosts (ein Mesh von `modeltool` ist heute eine Map `{mesh: 3}`), der
Host erfährt nicht, in welcher Zeile er gerufen wurde, und der Sammler meldet ihm nichts.

**Die Werkzeuge.** Sprachdienste für Editoren (`script::ls`: Vervollständigen, Hover, Signaturen, Umbenennen, Gliederung),
ein Formatierer, eine Debug-Schnittstelle (Haltepunkte, Schritte, Variablen, Ausdrücke), der Runner `jfs` mit Tests in
JFScript (`jfs --test`). JFNetworker hat darauf einen Editor mit Debugger gebaut.

**Wo es lebt.** Eigenes Repository, Crate `jfscript`, `no_std`, MIT oder Apache-2.0, nicht auf crates.io: Programme nehmen
es per git (Entscheidung des Nutzers vom 2026-11-09, JFLib docs/CRATES.md).

## Wer es nutzt und was jeder braucht

| Nutzer | Wofür | braucht vor allem | Determinismus |
| --- | --- | --- | --- |
| JFNetworker, Dienste | Server und Clients der virtuellen Hosts (`net`, `dns`, `http`, `db`, `disk`, `timer`) | Rückrufe, Anhalten mitten im Lauf, Brennstoff als simulierte CPU-Zeit | Pflicht: eine Simulation muss sich wiederholen lassen |
| JFNetworker, Seiten | `<script type="text/jfs">` im simulierten Browser (Dokument, `fetch`, Zeitgeber, Konsole) | Objekte für das Dokument, Ereignisse | Pflicht: virtuelle Zeit |
| `modeltool`, Spiele | Modelle und Karten aus Regeln und Bausteinen; eine KI schreibt sie | Vektoren, Host-Objekte für Meshes, benannte Argumente, Mathematik, Zeilen in Meldungen | Pflicht: dasselbe Modell überall, Hash als Cache-Schlüssel |
| JFDiagrammer (UI-21) | Automatisierung und Erweiterungen statt VBA und COM; Ereignisse (Auswahl, Zelle, Shape, Seite, Dokument) | ein Objektmodell (Dokument, Seite, Shape, Zelle), Ereignisse, ein Editor mit Debugger in der App, Sicherheit für Skripte in Dokumenten, Tempo bei großen Zeichnungen | nicht nötig; Uhr, Dateien und Dialoge über den Host |
| andere JF-Apps (Richtung des Nutzers) | Automatisierung, Skripte der Anwender, Erweiterungen | wie JFDiagrammer, je App ein eigenes Objektmodell | nicht nötig |

Spiellogik selbst ist in Rust geplant (`lib/game`, `apps/game1`), nicht in JFScript.

## Die Grundfragen

Zu jeder Frage die Möglichkeiten, was dafür und dagegen spricht, und eine Empfehlung. Die Antworten des Nutzers kommen in
eine Tabelle der Entscheidungen am Anfang, wie in JFLib GAME.md.

### G1. Eine Sprache für alle JF-Apps?

- **a) JFScript für alles:** Simulation, Modelle, Automatisierung, Erweiterungen.
- **b) JFScript für Simulation und Modelle, JavaScript für die Automatisierung der Apps.** Eine JavaScript-VM gibt es im
  eigenen Haus: `narsil-jsvm` aus narsil-engine (Interpreter, Basis-JIT für Windows x86-64, WebAssembly, die APIs des
  Webs).
- **c) Wie a), dazu später Erweiterungen in WebAssembly** (kompilierte Erweiterungen in jeder Sprache, für Tempo und für
  Dritte); narsil-engine hat eine eigene wasm-Engine (`narsil-wasm`).
- **d) Wie a), neue Syntax bewusst nah an JavaScript:** wo JFScript keinen Grund hat abzuweichen, folgt Neues dem
  Vorbild von JavaScript (`a ? b : c`, Zerlegen wie `let [x, y] = p` und `let {a, b} = m`, Kurzschreibweise für Felder).
  Bewusste Unterschiede bleiben: `int` neben `float` (JavaScript hat nur eine Zahl), `nil`, Maps statt Objekten mit
  Prototypen, Bereiche `0..n`.

Wo es laufen muss: JF-Apps laufen auch im Browser, auf Android und iOS. JFScript ist `no_std` und läuft dort schon
(`modeltool` hat in Node dieselbe Prüfsumme). `narsil-jsvm` baut heute nur für Windows, macOS und Linux (keine
Anpassung für wasm32, Android oder iOS, 142 Stellen mit `std::thread`, `std::time` oder `std::fs`).

Was in jedem Fall gebaut werden muss, unabhängig von der Sprache: das Objektmodell jeder App mit Ereignissen (G4), die
Rechte je Skript (G5) und der Editor mit Debugger in der App (G8). Die Wahl zwischen a und b ändert nur, welche VM
darunter liegt.

Für a: eine Sprache, ein Satz Werkzeuge (die Sprachdienste und der Debugger gibt es schon), volle Kontrolle über Sprache,
Rechte und Determinismus; die Syntax ist JavaScript schon nah, Menschen und KIs finden sich schnell hinein. Gegen a: ein
großer Ausbau (Objekte, Typen, Tempo), und Anwender lernen eine neue Sprache ohne fremde Bibliotheken.

Für b: JavaScript kennen viele, KIs schreiben es sehr sicher, die VM ist schnell. Gegen b: `narsil-jsvm` ist groß (rund
224 000 Zeilen), braucht `std` und fremde Crates (`temporal_rs`, ICU4X), ist auf das Web zugeschnitten und nicht
deterministisch; zwei Sprachen heißen zwei Werkzeugketten und zwei Rechtemodelle in jeder App. In die Bibliothek JFLib
dürfte es nicht (keine fremden Crates), nur in Apps.

**Empfehlung: a mit d**, c als spätere Ergänzung offenhalten. Drei der vier Nutzer brauchen den Determinismus, JFScript
läuft auf allen Zielen der JF-Apps, die Werkzeuge sind da, und die Nähe zu JavaScript nimmt b den größten Vorteil.

### G2. Determinismus: überall Pflicht?

Der Kern der Sprache bleibt ohne Uhr, Zufall und Dateien; was nicht deterministisch ist, kommt nur über den Host und
seine Rechte (wie heute bei `jfs`). Das passt zu allen Nutzern. Offen ist nur das Gleitkomma:

- **a) Software wie heute:** bitgleich, aber langsam.
- **b) Hardware für `+ − × ÷` und die Wurzel, NaN auf eine Form gebracht:** nach `num.rs` stimmen die Ziele für endliche
  Werte überein; JFLib `lib/physics` rechnet so und hat dieselbe Prüfsumme auf Windows, Linux, macOS, im Browser und auf
  Android. Die Mathematik (`sin`, `pow`, ...) bleibt fdlibm.
- **c) Zwei Betriebsarten:** deterministisch für Simulationen und Modelle, schnell für Apps.

Gemessen am 2026-11-23 mit `jfs` (Release, Ryzen 5 3600 unter Last, Median aus drei Läufen, je 3 Millionen Runden):

| Schleife | Zeit | je Runde |
| --- | ---: | ---: |
| leer (`s = i`) | 199 ms | 66 ns |
| drei Ganzzahl-Rechnungen | 232 ms | 77 ns |
| drei Gleitkomma-Rechnungen | 367 ms | 122 ns |
| `math.sin` und zwei Rechnungen (300 000 Runden) | 403 ms | 1,3 µs |

Eine Gleitkomma-Rechnung kostet in Software rund 18 ns statt rund 4 ns für eine ganzzahlige; in einer typischen Schleife
macht das das Ganze etwa anderthalbmal so langsam, weil die VM selbst (66 ns je Runde) den Großteil trägt. Teuer ist die
Mathematik: ein `sin` kostet über eine Mikrosekunde. Der größere Hebel für Tempo ist die VM (G6), das Gleitkomma der
zweite.

**Empfehlung: b**, vorher gegen die heutigen Tests geprüft (Flush-to-Zero darf in keinem Thread eines Hosts gesetzt
sein; NaN nach jeder Rechnung auf eine Form). Eine zweite Betriebsart (c) nur, wenn b nicht reicht: zwei Semantiken
derselben Sprache sind eine dauernde Fehlerquelle. Der Determinismus der Sprache selbst bleibt Pflicht: keine Uhr, kein
Zufall, keine Dateien im Kern, nur über den Host.

### G3. Typen

- **a) dynamisch bleiben.**
- **b) optionale Typangaben** (`fn area(r: float) -> float`), geprüft vom Compiler und den Sprachdiensten, ohne Änderung
  zur Laufzeit; helfen beim Vervollständigen, bei größeren Erweiterungen und einer KI.
- **c) statisch typisiert.**

Für Apps kommt der größte Nutzen von Typen nicht aus den Skripten, sondern aus dem Objektmodell des Hosts: Weiß der
Sprachdienst, dass `doc.page.shapes` Shapes liefert und ein Shape `text` und `cell(name)` hat, kann er vervollständigen
und Tippfehler melden, ohne dass das Skript selbst etwas angibt (wie TypeScripts `.d.ts` für JavaScript-Bibliotheken
oder die deklarierten Objekte von VBA). Das gehört zu den Beschreibungen des Hosts (G4, Wunsch 9) und braucht keine
neue Syntax.

Wenn später Typangaben im Skript kommen (b), dann in der Form von TypeScript (`fn area(r: float): float`, nach G1 d),
zur Laufzeit ohne Wirkung und nur vom Compiler und den Sprachdiensten geprüft; die Funktionen des Hosts prüfen ihre
Argumente ohnehin.

**Empfehlung: a jetzt; Typen zuerst in den Beschreibungen des Hosts (mit G4), Angaben im Skript (b) als spätere Stufe.**
c würde die heutigen Skripte von JFNetworker brechen und die Sprache für kleine Skripte schwer machen.

### G4. Objekte: Host-Objekte, eigene Typen, Operatoren, Ereignisse

Das ist die Frage, an der fast alle Wünsche hängen. Wie es heute ohne Host-Objekte aussieht, zeigen die Seiten-Skripte
von JFNetworker (`pagescript.rs`): ein Element ist die Map `{kind, node, tag}` (dasselbe Element ist nur für die letzten
paar berührten dieselbe Map), Text setzt `el.set_text(x)` statt `el.text = x` („jfs has no setters“), Ereignisse
kommen über `el.on(type, f)` und `el.off(type, f)`. In `modeltool` ist ein Mesh die Map `{mesh: 3}`, die das Skript
ändern kann und die der Host nie freigeben kann.

- **G4.1 Host-Objekte:** ein eigener Wert, den das Skript nicht öffnen kann (Typ und Id des Hosts). `obj.m(..)` geht an
  den Host, `type()`, `str()` und `print` fragen ihn, `==` vergleicht die Identität, der Sammler meldet freigegebene
  Werte (`Host::drop`). Alle Nutzer brauchen sie: Elemente in JFNetworker, Meshes in `modeltool`, Shapes und Zellen in
  JFDiagrammer. *Empfehlung: ja.*
- **G4.2 Eigenschaften lesen und setzen:** `shape.text` und `shape.text = "x"` gehen an den Host (lesen und setzen je
  Eigenschaft), wie in JavaScript und VBA. *Empfehlung: ja, für Host-Objekte.*
- **G4.3 Operatoren:** nur für Host-Typen (`a + b` fragt den Host, wenn `a` oder `b` ein Host-Wert ist; ebenso `==` für
  Werttypen wie Vektoren, die der Host so kennzeichnet). So wird ein Vektor in `modeltool` ein Host-Typ, ein Punkt in
  JFDiagrammer ebenso, ohne einen Vektortyp fest in die Sprache zu bauen. Keine Operatoren für Maps und Skript-Typen.
  *Empfehlung: so.*
- **G4.4 Eigene Typen im Skript:** heute Maps. Möglichkeiten: Maps bleiben; Records mit festen Feldern; Klassen wie in
  JavaScript (`class`, zuerst ohne Vererbung). *Empfehlung: Maps bleiben, bis Erweiterungen eigene Typen zeigen; dann
  `class` nach G1 d.*
- **G4.5 Ereignisse:** eine Form für alle Apps, die es in JFNetworker schon gibt: `obj.on("type", fn(e) {..})` und
  `obj.off("type", f)`, das Ereignis ein Host-Objekt; jeder Aufruf eines Handlers ist ein eigener Aufruf mit Brennstoff
  und Grenzen; ob ein Ereignis nach oben weitergeht (Shape, Seite, Dokument), legt der Host fest. *Empfehlung: so.*
- **G4.6 Lange Vorgänge** (Dateidialog, Netz, Warten): heute Rückrufe (`fetch(url, fn(err, resp))`). Nach G1 d wäre
  `await` der JavaScript-nahe Weg; die VM kann einen Lauf schon an jeder Stelle anhalten, das ist die Grundlage dafür.
  *Empfehlung: Rückrufe jetzt, `await` als Kandidat für den Durchgang über die Syntax (S3).*

**Empfehlung:** G4.1, G4.2, G4.3 und G4.5 als erste große Stufe (S2, die Host-Schnittstelle in zweiter Fassung, mit den
Typen der Beschreibungen nach G3); G4.4 und G4.6 später.

### G5. Sicherheit und Rechte

Skripte, die in Dokumenten mitkommen (wie VBA-Makros), sind das größte Risiko. JFLib hat dafür schon Bausteine:
`PERMISSIONS`, Fragen an den Anwender und eigene Rechte je Plugin (`plat::policy::on_behalf`).

Drei Arten von Skripten mit verschiedenem Vertrauen:

| Art | Woher | Vertrauen |
| --- | --- | --- |
| eigene Skripte | der Anwender schreibt sie in der App, sie liegen in seinem Ordner | wie der Anwender |
| Skripte in Dokumenten | kommen mit einer Datei (wie VBA in `.vsdm`, `.docm`) | keins, bis der Anwender zustimmt |
| Erweiterungen | installierte Pakete, später auch von Dritten | was ihr Manifest verlangt und der Anwender bei der Installation erlaubt |

- **G5.1 Rechte je Skript:** Jedes Skript läuft in einem eigenen Rahmen (wie Plugins: `plat::policy::on_behalf`). Was
  über das Objektmodell der App hinausgeht (Dateien, Netz, Programme, Zwischenablage, Drucken), prüft der Rahmen; die
  `PERMISSIONS` der App sind die obere Grenze. Fehlt ein Recht, fragt die App wie heute (`plat::policy`, Fragen an den
  Anwender). *Empfehlung: ja.*
- **G5.2 Skripte in Dokumenten** sind aus, wenn eine Datei geöffnet wird: eine Leiste sagt „Dieses Dokument enthält
  Skripte“ mit „Einmal ausführen“ und „Diesem Dokument vertrauen“ (gemerkt über den Hash der Skripte, ändert sich ein
  Skript, wird wieder gefragt). Auch erlaubt laufen sie im engsten Rahmen: das eigene Dokument, sonst nichts ohne
  Frage. Möglichkeiten dazu: nie ausführen (wie heute VBA in JFDiagrammer), nach Zustimmung (so), oder vertrauenswürdige
  Ordner wie in Office. *Empfehlung: nach Zustimmung, ohne vertrauenswürdige Ordner.*
- **G5.3 Erweiterungen** bringen ein Manifest mit den Rechten, die sie brauchen (in der Form von `PERMISSIONS`); der
  Anwender sieht sie bei der Installation und kann eine Erweiterung abschalten (FEATURES.md von JFDiagrammer, UI-20).
  *Empfehlung: ja, mit der Stufe für Erweiterungen.*
- **G5.4 Signaturen** für Erweiterungen und Dokument-Skripte, damit ein Herausgeber einmal vertraut werden kann. Braucht
  eine Kette von Zertifikaten und eine Stelle, die sie ausgibt. *Empfehlung: später, wenn es Erweiterungen von Dritten
  gibt.*
- **G5.5 Die App bleibt bedienbar:** jeder Aufruf hat Brennstoff und Speicher-Grenzen, auf dem Thread der Oberfläche
  dazu ein Budget je Bild (wie die Seiten-Skripte in JFNetworker, `CALL_FUEL` und `FRAME_FUEL`); ein Skript, das zu
  lange rechnet, endet mit einer Meldung, die App läuft weiter. *Empfehlung: ja, für alle Apps gleich.*

**Empfehlung:** G5.1, G5.2 und G5.5 mit der Stufe für Rechte (S4), G5.3 mit den Erweiterungen, G5.4 später. Der Rahmen
gehört in die Host-Schnittstelle von JFScript, die Rechte selbst bleiben in `plat::policy`.

### G6. Tempo

Heute: Interpreter, Gleitkomma in Software, Brennstoff je Befehl. Für Automatisierung über Tausende Shapes oder schwere
Schleifen in Modellen kann das zu langsam werden.

- Messen mit echten Skripten je Nutzer, bevor optimiert wird. Vorschlag für Zielwerte, als Prüfungen in `scripttest`
  und in den Apps:
  - JFDiagrammer: ein Skript geht über 10 000 Shapes und setzt bei jedem eine Eigenschaft, in unter 0,5 s auf der Iris
    Xe-Klasse (ein Shape kostet dann höchstens 50 µs; nach der Messung von G2 trägt die VM rund 70 ns je Schleifenrunde,
    der Aufruf des Hosts ist noch nicht gemessen).
  - `modeltool`: der Skript-Teil eines Modells mit 1000 Teilen in unter 1 s (die Geometrie rechnet Rust).
  - Ein Handler eines Ereignisses (Auswahl geändert) in unter 5 ms, damit die Oberfläche nicht stockt.
  - JFNetworker: unverändert (Brennstoff ist dort simulierte Zeit).
- Dann, wo die Ziele verfehlt werden: Gleitkomma in Hardware (G2 b), schnellere VM (Register statt Stapel,
  Zwischenspeicher für Feldzugriffe), billigere Aufrufe des Hosts.
- **Kein JIT:** iOS und der Browser erlauben keinen, und die Komplexität ist groß.

### G7. Verträglichkeit, Versionen, Ort

- **G7.1 Bestehende Skripte:** Die `.jfs` der Anwender von JFNetworker (in ihren Projekten) müssen weiterlaufen. Neues
  ist frei (neue Syntax, neue Funktionen, Host-Objekte); was die Bedeutung von bestehendem Code ändert, nur mit einer
  Version der Sprache (eine Angabe je Datei oder Projekt) und einer automatischen Umstellung durch den Formatierer.
  Möglichkeiten: nie etwas brechen; brechen nur mit Version und Umstellung; brechen frei, solange es wenige Anwender
  gibt. *Empfehlung: nur mit Version und Umstellung, und so selten wie möglich.*
- **G7.2 `int / int`:** schneidet heute ab (`1 / 3` ist 0, wie in C, Rust und Go), in Geometrie ein leiser Fehler
  (Wunsch 6). JavaScript kennt nur eine Zahl, `/` gibt dort immer eine Kommazahl; `//` scheidet aus, das ist in JFScript
  wie in JavaScript ein Kommentar. Möglichkeiten: bleibt, dazu eine Warnung der Sprachdienste, wo zwei Ganzzahlen geteilt
  werden; `/` gibt immer `float` und `int.div(a, b)` teilt ganzzahlig (ein Bruch nach G7.1). *Empfehlung: bleibt, mit der
  Warnung; kein Bruch für eine Feinheit.*
- **G7.3 Name:** JFScript und die Endung `.jfs` bleiben.
- **G7.4 Ort:** das eigene Repository bleibt, Programme nehmen JFScript per git (Entscheidung vom 2026-11-09). crates.io
  wird nur nötig, wenn ein Crate von JFLib selbst JFScript braucht (ein Crate auf crates.io darf nicht von git
  abhängen); Apps in eigenen Repos können git nehmen, wie JFNetworker es tut. Das lässt sich vermeiden, wenn JFLib nie
  von JFScript abhängt: nach G8 liegen alle Werkzeuge für Skripte im Repository von JFScript, JFLib liefert nur
  allgemeine Widgets. *Empfehlung: bei git bleiben, JFLib frei von JFScript halten;
  crates.io neu entscheiden, wenn die Host-Schnittstelle in zweiter Fassung (S2) stabil ist oder Dritte JFScript nutzen
  sollen.*

### G8. Werkzeuge für Anwender

Ein Ersatz für VBA braucht in der App, was der VBA-Editor bietet: Editor, Debugger, Konsole, eine Liste der Skripte und
einen Objektkatalog. Was es schon gibt:

- **Der Editor** ist in JFLib und sprachneutral: `ui.code_editor` mit Hervorhebung, Diagnosen, Vervollständigen, Hover
  und Signaturhilfe über eine Schnittstelle (`code_assist.rs`), einer Spalte für Haltepunkte, der Zeile des Debuggers
  (`ui.code_reveal`) und Ereignissen für Definition, Umbenennen und Formatieren. `editortest` zeigt ihn mit JFScript.
- **Debugger, Konsole und Skriptverwaltung** hat JFNetworker für sich gebaut (`src/workspace/ide`, rund 4900 Zeilen mit
  Explorer, Ausgabe, Terminal, Problemen, Debugger und Tests), eng an seine Geräte gebunden.

Die Teilfragen:

- **G8.1 Wo die Werkzeuge liegen:** im Repository von JFScript, in einem eigenen Crate neben der Sprache (Arbeitsname
  `jfscript-ide`): Debugger-Ansicht (Haltepunkte, Aufrufe, Variablen als Baum, Beobachten, Schritte), Konsole mit
  Eingabezeile, das Fenster „Skripte“ (G8.2), der Objektkatalog (G8.3) und die Anbindung an die Rechte über
  `plat::policy` (G5). Er setzt sich aus den allgemeinen Widgets von JFLib zusammen (`ui.code_editor` mit seiner
  Schnittstelle für Sprachdienste, die auch Rust, JSON und Markdown bedient; Bäume, Listen, Text) und nimmt `jflib` von
  crates.io wie JFScript heute. Apps nehmen ihn per git, auch JFDiagrammer, solange es im Repository von JFLib liegt
  (wie `editortest` und `modeltool` die Sprache). In JFLib kommt nichts nur für Skripte; fehlt ein allgemeines Widget,
  kommt es als gewöhnliche Funktion von `lib/ui`. *Entschieden so (der Nutzer, 2026-11-23); eine erste Fassung hatte
  sprachneutrale Bausteine für Debugger und Konsole in JFLib vorgesehen.*
- **G8.2 Ein Fenster „Skripte“** für alle Apps, im selben Crate: die Skripte des Anwenders, des Dokuments und der
  Erweiterungen, ausführen, an Schaltflächen und Tastenkürzel hängen, ein- und ausschalten (mit den Rechten aus G5); die
  App gibt ihr Objektmodell und ihre Orte dazu. *Empfehlung: ja, als gemeinsamer Baustein, den JFDiagrammer als erste
  App nimmt.*
- **G8.3 Objektkatalog:** alle Objekte, Eigenschaften, Methoden und Ereignisse einer App zum Nachschlagen, erzeugt aus
  denselben Beschreibungen wie die Sprachdienste (G3); daraus auch die Referenz der App als Seite. *Empfehlung: ja.*
- **G8.4 Makro-Rekorder:** Bedienschritte als Skript aufzeichnen, wie in VBA. Geht nur, wenn jeder Befehl der App einen
  Aufruf im Objektmodell hat. *Empfehlung: später, je App; die Objektmodelle von Anfang an so entwerfen, dass jeder
  Befehl einen Aufruf hat.*
- **G8.5 JFNetworker** kann später auf die gemeinsamen Bausteine wechseln, muss aber nicht. *Empfehlung: offenlassen.*

### G9. Sprache der Meldungen

Heute: Schlüsselwörter und die Namen der Bibliothek englisch (`let`, `fn`, `str.split`), Meldungen des Compilers und der
Laufzeit deutsch („Unbekannte Funktion …“, „Hier fehlt ein Doppelpunkt“), die Texte der Sprachdienste für die Bibliothek
deutsch (`ls::library_doc`). Die JF-Apps sind mindestens zweisprachig (`locales/ui.tsv` mit `de` und `en`), JFDiagrammer
plant fünf Sprachen der Oberfläche.

- **G9.1 Meldungen:** Möglichkeiten: deutsch bleiben; nur englisch; in der Sprache der App. *Empfehlung: in der Sprache
  der App.* JFScript bringt eine eigene Tabelle der Meldungen mit (zuerst `de` und `en`, in der Form der Übersetzungen
  von JFLib, damit weitere Sprachen dazukommen), der Host sagt, welche Sprache gilt; ohne Angabe Deutsch wie heute, so
  bleiben die Tests und JFNetworker unverändert.
- **G9.2 Texte der Sprachdienste** (Hover, Vervollständigen, Objektkatalog): auf demselben Weg; die Beschreibungen des
  Hosts bringt jede App in ihren Sprachen mit. *Empfehlung: ja.*
- **G9.3 Namen in den Objektmodellen** (Funktionen, Eigenschaften, Ereignisse, Parameter): englisch wie die Sprache, die
  Bibliothek, VBA und die ShapeSheet-Namen von Visio. Mit benannten Argumenten (Wunsch 4) werden auch die Namen der
  Parameter Teil der Schnittstelle; `modeltool` hat heute deutsche (`m.bevel(breite, segmente, achse)`), die wären dann
  umzustellen, bevor benannte Argumente kommen. Schlüsselwörter werden nie übersetzt. *Empfehlung: englisch.*

## Die Wünsche aus dem Modellieren, eingeordnet

Aus JFLib GEOM.md, „Was JFScript braucht“, in der Rangfolge des Bewertungslaufs:

| Wunsch | hängt an | Art |
| --- | --- | --- |
| 1 Vektoren mit Operatoren | G4 (Host-Typen, Operatoren) | Objektmodell |
| 2 Host-Objekte | G4 | Objektmodell, Kern |
| 3 Stelle des Aufrufs, Warnungen des Hosts | nichts | Host-Schnittstelle, klein |
| 4 benannte Argumente | Syntax (G7) | Sprache |
| 5 mehr Mathematik, Winkel in Grad | nichts | Bibliothek |
| 6 `int / int` schneidet ab | G7 (Version der Sprache) | Semantik |
| 7 Bereiche mit Schritt, `linspace` | nichts | Bibliothek |
| 8 Zerlegen beim Zuweisen | Syntax (G7) | Sprache |
| 9 Beschreibungen der Host-Funktionen für die Sprachdienste | nichts | Host-Schnittstelle |
| 10 Bedingungsausdruck, Liste aus einer Schleife | Syntax (G7) | Sprache |

3, 5, 7 und 9 sind in jeder Richtung richtig und ändern nichts Bestehendes. 4, 8 und 10 passen zu jeder Richtung, sollten
aber in einem Durchgang über die Syntax entworfen werden, nicht einzeln. 1, 2 und 6 warten auf G4 und G7.

## Stufen

Nach den Entscheidungen G1 bis G9:

| Stufe | Inhalt | Erste Nutzer |
| --- | --- | --- |
| S0 | Dieses Papier | – |
| S1 | Was in jeder Richtung richtig ist und nichts Bestehendes ändert: die Stelle des Aufrufs und Warnungen für den Host (`cx.position()`, `cx.warn`), Mathematik (`tan`, `asin`, `acos`, `atan`, `atan2`, `hypot`, Winkel in Grad, bei 90° exakt), Bereiche mit Schritt und `linspace`, die Warnung bei `int / int` (G7.2); in `modeltool` englische Namen der Parameter (G9.3). **In JFScript fertig am 2026-11-24:** `cx.position()`, `cx.warn` und `vm.take_warnings()`; `math.tan`, `asin`, `acos`, `atan`, `atan2` (fdlibm, Bit für Bit wie Javas `StrictMath`, `atan2` mit der Korrektur von FreeBSD), `math.hypot` (genau gerundet, beliebig viele Werte), `math.sin_deg`, `cos_deg`, `tan_deg` (exakt, wo das Ergebnis eine Gleitkommazahl ist); `list.range(start, end, step)` und `list.linspace(start, end, n)` im Modul `list`, also ohne neue globale Namen; die Warnung bei `int / int` in `check`. Der Teil in `modeltool` folgt in JFLib. | `modeltool`, JFNetworker |
| S2 | Die Host-Schnittstelle in zweiter Fassung (G4): Host-Objekte, Eigenschaften, Operatoren für Host-Typen, Ereignisse mit `on` / `off`; die Beschreibungen des Hosts mit Typen für die Sprachdienste und den Objektkatalog (G3, G8.3). **In JFScript fertig am 2026-11-24** (docs/HOST.md): Host-Objekte `Value::Host` mit dem Typ und der Id des Hosts (`cx.new_object`: ein Objekt je Id, solange ein Skript es hält, `==` vergleicht die Identität, `Host::drop`, wenn der Sammler es freigibt) und Werttypen (`cx.new_value`: die Daten liegen in der VM, `==` vergleicht Typ und Daten); `Host::get`, `set`, `method`, `op`, `listen`, `to_str` und `drop` mit Vorgaben, so bleiben bestehende Hosts unverändert; `obj.name`, `obj.name = x` und `+=` über den Host, Maps behalten ihre Felder; `+ - * / %`, `< <= > >=` und das Minus fragen den Host, wenn eine Seite ein Host-Objekt ist, `==` bleibt in der VM; `type()`, `print`, `str()` und `${..}` fragen ihn; Ereignisse mit `obj.on` / `obj.off`, die Funktionen hält die VM (`vm.listeners`, `vm.emit`: jeder Handler ein eigener Aufruf mit Brennstoff und Grenzen, `cx.emit` in einem Aufruf, `remove_listeners`), ob es aufsteigt, entscheidet der Host; Fehler der Eigenschaften und Operatoren sind fangbar mit Zeile und Spalte; der Code ohne Host-Objekte läuft so schnell wie vorher. Die Beschreibungen `script::Api` (Funktionen, Werte und Namensräume, Typen mit Eigenschaften, Methoden, Ereignissen und Operatoren, Signaturen in der Form von TypeScript, `Api::from_items` für die bisherigen `HostItem`) in den Sprachdiensten (`ls::Context`: Vervollständigen nach dem Punkt auch über Ketten, Hover mit Typen, Signaturen, `ls::check` warnt vor unbekannten Mitgliedern) und als Objektkatalog (`Api::markdown`, `Api::catalogue` für eine Oberfläche). Der Teil in `modeltool` (Vektoren und Meshes als Host-Objekte) und in JFNetworker folgt in ihren Repositories. | Seiten in JFNetworker (Elemente als Host-Objekte), Meshes und Vektoren in `modeltool` |
| S3 | Ein Durchgang über die Syntax nach G1 d: Bedingungsausdruck, benannte Argumente, Vorgabewerte, Zerlegen, Kurzform für Felder, Listen aus Schleifen (Formen in der Tabelle am Anfang), `await` für lange Vorgänge des Hosts (G4.6); Meldungen in der Sprache der App (G9); die Lücken, die `modeltool` an S2 gefunden hat (Warnungen aus `compile_with`, optionale Parameter, Überladungen und Unions in `script::Api`, der Typname von Host-Objekten, Vorschläge für unbekannte Namen). Versionen der Sprache erst mit der Freigabe (G7.1, ergänzt am 2026-11-27) | alle |
| S4 | Rechte je Skript und Skripte in Dokumenten (G5.1, G5.2, G5.5); der Crate `jfscript-ide` im Repository von JFScript mit Debugger, Konsole und dem Fenster „Skripte“ (G8.1, G8.2) | JFDiagrammer |
| S5 | Tempo gegen die Zielwerte (G6), Gleitkomma in Hardware, wenn die Tests dieselben Bits zeigen (G2) | alle |
| S6 | Die Automatisierung von JFDiagrammer (FEATURES.md UI-21) auf S2 bis S4 | JFDiagrammer |
| später | optionale Typangaben (G3 b), `class` (G4.4), Erweiterungen mit Manifest (G5.3), Signaturen (G5.4), Makro-Rekorder (G8.4), Erweiterungen in WebAssembly (G1 c), crates.io (G7.4) | – |

Jede Stufe hält die Skripte von JFNetworker lauffähig (`scripttest`, die Fälle von `jfs`, die Tests von JFNetworker) und
die Prüfsumme von `modeltool` gleich, solange nicht ausdrücklich eine Version der Sprache sie ändert.

## Zu entscheiden

Nichts mehr: G1 bis G9 und die Reihenfolge sind entschieden (Tabelle am Anfang). S1 und S2 sind in JFScript fertig,
die Teile in `modeltool` (und für S2 in JFNetworker, nach G8.5 freiwillig) folgen.
