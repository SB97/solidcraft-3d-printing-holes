# 3D Printing Holes – SolidCraft-Plugin

Löcher, die sich gut 3D-drucken lassen. Erste Funktion: **Teardrop**.

Ein waagrechtes Loch in einer senkrechten Wand hat oben eine flache Decke, die beim Druck durchhängt.
Der Teardrop ersetzt diese Decke durch zwei Flanken mit 45° Überhang, die sich in einer Spitze treffen.
So druckt das Loch ohne Stützmaterial.

## Bedienung

1. Im Sketch einen normalen Kreis zeichnen und bemaßen, z. B. Radius `hole_r`.
2. Den Kreis auswählen (mehrere gehen auch).
3. Plugin-Eintrag `teardrop` ausführen.

Der Kreis wird zu einem Bogen um den alten Mittelpunkt und zwei tangentialen Flanken bis zur Spitze.
Das Ergebnis ist ganz normale Sketch-Geometrie mit Constraints:

- **Konstruktionslinie Mitte → Spitze:** zeigt im Sketch dauerhaft, wo „oben“ ist.
- **Radius:** Die Radius- bzw. Durchmesserbemaßung des Kreises geht samt Ausdruck auf den Bogen über. Wer den Parameter ändert, ändert den ganzen Teardrop mit.
- **Spitze:** Ein Winkel-Constraint hält sie. Ändert sich der Radius, wandert sie automatisch mit, ohne dass das Plugin erneut laufen muss.

### Felder

| Feld | Standard | Bedeutung |
|---|---|---|
| `circles` | leer = Auswahl | Kreis-IDs (`"c1"` oder `{id, sketch}`) |
| `up` | `auto` | `auto` = Welt-+Z (Druckrichtung) in die Sketch-Ebene projiziert; oder `+x`, `-x`, `+y`, `-y`, Winkel in Grad zur Sketch-X-Achse |
| `overhang_angle` | `45` | steilster Überhang zur Horizontalen |
| `tip_height` | leer | Override: Abstand Mitte → Spitze, Zahl (mm) oder Ausdruck mit `r` = Radius, z. B. `"1.3*r"` |
| `flat_top` | leer | Spitze flach abschneiden im Abstand vom Mittelpunkt, z. B. `"r"` |

### Formel

- Spitze: **h = r / cos(α)**. Für α = 45° ergibt das **h = r·√2 ≈ 1,414·r**.
- Fläche bei 45°: r²·(3π/4 + 1).
- Mit `tip_height` (h) wird der Flankenwinkel zu φ = acos(r/h).

`up: auto` verweigert Sketches auf einer waagrechten Ebene, denn dort steht das Loch senkrecht und braucht keinen Teardrop.
Constraints, die am Kreis hingen (z. B. tangential zu etwas), gehen verloren. Das Plugin meldet sie als Warnung.
Am besten wandelt man den Kreis um, bevor man extrudiert.

## Installieren

- **GUI:** *Preferences › Plugins › Install plugin…* und `dist/printing-holes.zip` wählen.
- **Manuell:** Den Ordner `dist/printing-holes/` nach `%LOCALAPPDATA%\SolidCraft\plugins\printing-holes\` kopieren und SolidCraft neu starten.

Ausführen geht per `plugin.inspect` (liefert den Freigabe-Hash) und dann `plugin.run` (über MCP, Agent-Chat oder CLI):

```json
[{"command":"plugin.inspect","params":{"path":"dist/printing-holes"}}]
[{"command":"plugin.run","params":{"path":"dist/printing-holes","approval":"<hash>","entry":"teardrop","params":{}}}]
```

Die Manifest-Slots (Ribbon/Menü „Sketch“) sind deklariert.
Ob SolidCraft daraus schon einen Button mit Formular zeichnet, hängt von der App-Version ab: Plugin-Formulare sind laut RFC 0002 Phase 4.

## Bauen

Voraussetzungen:

- rustup; die Toolchain `stable-x86_64-pc-windows-gnu` wird über `rust-toolchain.toml` automatisch gewählt und braucht keine Visual Studio Build Tools
- [wasm-tools](https://github.com/bytecodealliance/wasm-tools) im `PATH`

```powershell
.\scripts\build.ps1          # -> dist/printing-holes/ und dist/printing-holes.zip
cargo test                   # Geometrie- und Parser-Tests
node tests/e2e/run.mjs       # End-to-End gegen solidcraft-cli (headless)
```

## Aufbau

| Datei | Inhalt |
|---|---|
| [src/geometry.rs](src/geometry.rs) | reine Teardrop-Geometrie, Projektion von „oben“ |
| [src/teardrop.rs](src/teardrop.rs) | Eingaben, Kreis lesen, Geometrie und Constraints über App-Befehle erzeugen |
| [src/lib.rs](src/lib.rs) | Entry-Dispatch und WIT-Bindings (`craft:plugin@0.1.0`) |
| [wit/plugin.wit](wit/plugin.wit) | festgehaltene Kopie der SolidCraft-Plugin-WIT |
| [plugin/plugin.json](plugin/plugin.json) | Manifest: Entry, Eingabeschema, erlaubte Befehle |
