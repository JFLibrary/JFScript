# JFScript: Richtung (Entwurf eines Entscheidungspapiers)

Stand 2026-11-23, Entwurf zur Besprechung. **Nichts hier ist entschieden.** Das Papier soll festlegen, wofür JFScript da
ist und wie es wächst, bevor eine einzelne App es in ihre Richtung ausbaut. Bis dahin bleibt JFScript unverändert;
`modeltool`, JFNetworker und die Tests kommen mit der heutigen Sprache aus.

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

**Empfehlung: b**, vorher gemessen und gegen die heutigen Tests geprüft (Flush-to-Zero darf in keinem Thread eines
Hosts gesetzt sein). Eine zweite Betriebsart (c) nur, wenn b nicht reicht: zwei Semantiken derselben Sprache sind eine
dauernde Fehlerquelle.

### G3. Typen

- **a) dynamisch bleiben.**
- **b) optionale Typangaben** (`fn area(r: float) -> float`), geprüft vom Compiler und den Sprachdiensten, ohne Änderung
  zur Laufzeit; helfen beim Vervollständigen, bei größeren Erweiterungen und einer KI.
- **c) statisch typisiert.**

**Empfehlung: a jetzt, b als spätere Stufe.** c würde die heutigen Skripte von JFNetworker brechen und die Sprache für
kleine Skripte schwer machen.

### G4. Objekte: Host-Objekte, eigene Typen, Operatoren, Ereignisse

Das ist die Frage, an der fast alle Wünsche hängen.

- **Host-Objekte** (ein Wert, den das Skript nicht öffnen kann: Typ und Id des Hosts; `obj.m(..)` geht an den Host;
  `type()` und `print` fragen ihn; der Sammler meldet freigegebene Werte). Alle Nutzer brauchen sie: Knoten des
  Dokuments in JFNetworker, Meshes in `modeltool`, Shapes und Zellen in JFDiagrammer.
- **Operatoren für Host-Typen** (`a + b` ruft den Host, wenn `a` ein Host-Wert ist): so wird ein Vektor in `modeltool`
  ein Host-Typ, ein Punkt in JFDiagrammer ebenso, ohne einen Vektortyp fest in die Sprache zu bauen.
- **Eigene Typen im Skript** (Records oder Klassen): heute Maps. Möglichkeiten: Maps bleiben; Records mit festen Feldern;
  Klassen mit Methoden.
- **Ereignisse:** eine Form für alle Apps (`doc.on("selection", fn(e) {..})` oder Felder wie `on_change`), vom Host
  ausgelöst, mit Brennstoff und Grenzen wie jeder Aufruf.

**Empfehlung:** Host-Objekte als erste große Stufe, Operatoren nur für Host-Typen, Ereignisse als Konvention der
Host-Schnittstelle darüber; eigene Typen im Skript erst, wenn Erweiterungen sie zeigen (dann Records vor Klassen).

### G5. Sicherheit und Rechte

Skripte, die in Dokumenten mitkommen (wie VBA-Makros), sind das größte Risiko. JFLib hat dafür schon Bausteine:
`PERMISSIONS`, Fragen an den Anwender und eigene Rechte je Plugin (`plat::policy::on_behalf`).

- Jedes Skript bekommt einen eigenen Rahmen von Rechten; Funktionen des Hosts, die etwas außerhalb der App tun (Dateien,
  Netz, Programme), prüfen ihn.
- Skripte aus Dokumenten laufen nie von selbst: erst nach einer Zustimmung je Dokument oder je Herausgeber; Signaturen
  später.
- Die Grenzen der VM (Speicher, Brennstoff) bleiben für jedes Skript an.

**Empfehlung:** so; der Rahmen gehört in die Host-Schnittstelle von JFScript, die Rechte selbst in `plat::policy`.

### G6. Tempo

Heute: Interpreter, Gleitkomma in Software, Brennstoff je Befehl. Für Automatisierung über Tausende Shapes oder schwere
Schleifen in Modellen kann das zu langsam werden.

- Messen mit echten Skripten je Nutzer, bevor optimiert wird.
- Dann: Gleitkomma in Hardware (G2 b), schnellere VM (Register statt Stapel, Zwischenspeicher für Feldzugriffe).
- **Kein JIT:** iOS und der Browser erlauben keinen, und die Komplexität ist groß.

### G7. Verträglichkeit, Versionen, Ort

- Die `.jfs` der Anwender von JFNetworker (in ihren Projekten) müssen weiterlaufen: Änderungen der Semantik (etwa
  `int / int`, das heute abschneidet) nur mit einer Version der Sprache je Datei oder Projekt und einer Umstellung durch
  den Formatierer.
- Name JFScript und Endung `.jfs` bleiben.
- **Ort:** das eigene Repository bleibt. Wird JFScript die Sprache aller Apps, sollte es auf crates.io (ein Crate auf
  crates.io darf nicht von git abhängen; JFDiagrammer und andere Apps ziehen später in eigene Repos). Das ändert die
  Entscheidung vom 2026-11-09.

### G8. Werkzeuge für Anwender

Ein Ersatz für VBA braucht in der App einen Editor mit Debugger und eine Konsole (wie den VBA-Editor). JFNetworker hat
das schon für sich gebaut.

**Empfehlung:** daraus ein gemeinsamer Baustein für alle Apps (Editor mit den Sprachdiensten, Konsole, Haltepunkte und
Variablen), in JFLib neben dem Editor-Widget; die Apps geben ihr Objektmodell mit Beschreibungen für die Sprachdienste
dazu.

### G9. Sprache der Meldungen

Die Fehlermeldungen sind deutsch, die Schlüsselwörter englisch. Für eine Skriptsprache aller Apps, auch für Anwender in
anderen Sprachen, sollten die Meldungen über die Übersetzungen der App kommen (wie die Texte von `lib/ui`).

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

## Mögliche Stufen nach den Entscheidungen

| Stufe | Inhalt | Erste Nutzer |
| --- | --- | --- |
| S0 | Dieses Papier: die Entscheidungen G1 bis G9 | – |
| S1 | Was in jeder Richtung richtig ist: Stelle des Aufrufs und Warnungen, Mathematik, Bereiche, Beschreibungen des Hosts für die Sprachdienste | `modeltool`, JFNetworker |
| S2 | Host-Objekte, Operatoren für Host-Typen, Ereignisse (die Host-Schnittstelle, zweite Fassung) | Seiten in JFNetworker, Meshes in `modeltool` |
| S3 | Ein Durchgang über die Syntax: Bedingungsausdruck, benannte Argumente, Zerlegen, Listen aus Schleifen; Versionen der Sprache | alle |
| S4 | Rechte je Skript, Skripte in Dokumenten, der gemeinsame Editor mit Debugger | JFDiagrammer |
| S5 | Tempo (gemessen, Gleitkomma nach G2), optionale Typen (G3) | alle |
| S6 | Die Automatisierung von JFDiagrammer (UI-21) auf S2 bis S4 | JFDiagrammer |

Jede Stufe hält die Skripte von JFNetworker lauffähig (`scripttest`, die Fälle von `jfs`, die Tests von JFNetworker) und
die Prüfsumme von `modeltool` gleich, solange nicht ausdrücklich eine Version der Sprache sie ändert.

## Zu entscheiden

1. **G1:** JFScript als die eine Skriptsprache aller JF-Apps (a), oder JavaScript über `narsil-jsvm` für die
   Automatisierung der Apps (b)? WebAssembly für Erweiterungen später offenhalten (c)?
2. **G2:** Gleitkomma in Hardware mit gleicher Bitgenauigkeit prüfen (b), oder Software bleibt?
3. **G3:** dynamisch bleiben, optionale Typen später?
4. **G4:** Host-Objekte als erste große Stufe, Operatoren nur für Host-Typen?
5. **G5:** Skripte aus Dokumenten nur nach Zustimmung, Rechte je Skript über `plat::policy`?
6. **G7:** JFScript auf crates.io, wenn es die Sprache aller Apps wird?
7. **G8:** ein gemeinsamer Skript-Editor mit Debugger in JFLib?
8. **Reihenfolge:** S1 sofort nach den Entscheidungen, oder erst nach S2?
