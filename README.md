# 3D Printing Holes – SolidCraft-Plugin

Löcher, die sich gut 3D-drucken lassen. Erste Funktion: **Teardrop**.

Ein waagrechtes Loch in einer senkrechten Wand hat oben eine flache Decke, die beim Druck durchhängt.
Der Teardrop ersetzt diese Decke durch zwei tangentiale Flanken, die sich in einer kleinen Spitze treffen.
Standardmäßig liegt die Spitze nur **5 % des Radius** über dem Kreis. Das reicht, um das Durchhängen
auszugleichen, und das Loch bleibt praktisch rund. Der klassische 45°-Teardrop ist weiterhin möglich.


## Bedienung

1. Im Sketch einen normalen Kreis zeichnen und bemaßen, z. B. Radius `hole_r`.
2. Mit **Esc** das aktive Sketch-Werkzeug beenden und den Kreis auswählen (mit Shift auch mehrere).
3. **Plugins › 3D Printing Holes › Teardrop…** öffnen (oder im Ribbon **Model › Plugins › Teardrop**).
4. Im Formular die Felder prüfen, **Run** drücken und beim ersten Mal das Paket freigeben.

Der Sketch des Kreises muss dafür nicht aktiv sein. Gehört der ausgewählte Kreis zu einem anderen Sketch
als dem gerade bearbeiteten (SolidCraft meldet ihn dann als `external_sketch_curve`), schließt das Plugin den aktiven Sketch,
wandelt den Kreis in seinem eigenen Sketch um und öffnet danach wieder den Sketch, in dem man vorher war.

Findet das Plugin keinen ausgewählten Kreis, nennt die Fehlermeldung, was stattdessen ausgewählt war.
Alternativ wandelt **Circles = `all in sketch`** alle Kreise des offenen Sketches um.

Das Formular erzeugt SolidCraft selbst aus dem Eingabeschema im Manifest.
Der ganze Lauf ist ein Undo-Schritt.

Der Kreis wird zu einem Bogen um den alten Mittelpunkt und zwei tangentialen Flanken bis zur Spitze.
Das Ergebnis ist ganz normale Sketch-Geometrie mit Constraints:

- **Konstruktionslinie Mitte → Spitze:** zeigt im Sketch dauerhaft, wo „oben“ ist.
- **Radius:** Die Radius- bzw. Durchmesserbemaßung des Kreises geht samt Ausdruck auf den Bogen über. Wer den Parameter ändert, ändert den ganzen Teardrop mit.
- **Spitze:** Eine Längenbemaßung mit Ausdruck hält sie, z. B. `1.05 * (hole_r)`. Ändert sich der Radius, wandert sie automatisch mit, ohne dass das Plugin erneut laufen muss.

### Felder im Formular

| Feld | Standard | Bedeutung |
|---|---|---|
| **Circles** (`scope`) | `selected` | `selected` = die ausgewählten Sketch-Kreise; `all in sketch` = alle (Nicht-Konstruktions-)Kreise des offenen Sketches |
| **Up** (`up`) | `auto` | Richtung der Spitze: `auto` = Welt-+Z (Druckrichtung) in die Sketch-Ebene projiziert; `+x`, `-x`, `+y`, `-y` = Sketch-Achsen; `angle` = Winkel aus dem nächsten Feld |
| **Up angle** (`up_angle`) | `90` | nur bei Up = `angle`: Winkel in Grad zur Sketch-X-Achse (90 = Sketch-+Y) |
| **Warn on horizontal sketch** (`warn_horizontal`) | an | Warnung, wenn Up = `auto` auf einem waagrechten Sketch keine Richtung findet (siehe unten) |
| **Tip above circle (% of r)** (`tip_extra`) | `5` | wie weit die Spitze über dem Kreis liegt, in Prozent des Radius |
| **Tip height** (`tip_height`) | leer | Override: Abstand Mitte → Spitze, in mm oder als Ausdruck mit `r` = Radius, z. B. `r + 0.3 mm` oder `r*sqrt(2)` (klassisch 45°) |
| **Flat top at** (`flat_top`) | leer | Spitze flach abschneiden im Abstand vom Mittelpunkt, z. B. `r` |

Nur per Skript/CLI (nicht im Formular): `circles` (Kreis-IDs `"c1"` oder `{id, sketch}`, leer = Auswahl) und `sketch` (Sketch-ID/-Name, wenn kein Sketch offen ist).
`up` nimmt dort auch direkt einen Winkel, z. B. `"300"`. `overhang_angle` (Grad zur Horizontalen) baut den Teardrop über einen Winkel-Constraint statt über die Höhe und hat Vorrang vor `tip_extra`.

### Formel

- Standard: **h = r·(1 + p/100)** mit p = `tip_extra`, also **h = 1,05·r** bei 5 %.
- Die Flanken berühren den Kreis im Winkel φ = acos(r/h) von „oben“ aus (bei 5 %: φ ≈ 17,8°) und stehen φ zur Horizontalen.
  Die Spitze überbrückt also nur eine kurze Sehne von 2·r·sin φ (≈ 0,61·r) statt der ganzen Lochbreite.
- Fläche: r²·(π − φ) + r·h·sin φ; bei 5 % sind das nur ≈ 0,3 % mehr als der Kreis.
- Klassisch mit Überhangwinkel α: h = r / cos(α), bei 45° also h = r·√2 ≈ 1,414·r.

Das Durchhängen ist eher eine feste Strecke (ein, zwei Schichthöhen) als ein Anteil des Radius.
Für sehr große oder sehr kleine Löcher passt deshalb ein Ausdruck wie `tip_height = r + 0.3 mm` oft besser.

**Waagrechte Sketches:** Die CAD-Lage ist nicht immer die Drucklage.
Liegt der Sketch waagrecht, liefert Up = `auto` keine Richtung, deshalb zeigt die Spitze dann nach Sketch-+Y.
Das Plugin warnt dabei; mit **Warn on horizontal sketch** = aus bleibt es still.
Wird das Teil anders gedruckt, als es modelliert ist, setzt man Up direkt.
Constraints, die am Kreis hingen (z. B. tangential zu etwas), gehen verloren. Das Plugin meldet sie als Warnung.
Am besten wandelt man den Kreis um, bevor man extrudiert.

## Installieren

- **GUI:** *Plugins › Install plugin…* (oder *Preferences › Plugins*) und `dist/printing-holes.zip` wählen. Eine bereits installierte Version wird dabei ersetzt.
- **Manuell:** Den Ordner `dist/printing-holes/` nach `%LOCALAPPDATA%\SolidCraft\plugins\printing-holes\` kopieren und SolidCraft neu starten.

Ohne GUI (Skript, MCP, Agent-Chat) geht das Ausführen per `plugin.inspect` (liefert den Freigabe-Hash) und dann `plugin.run` (über MCP, Agent-Chat oder CLI):

```json
[{"command":"plugin.inspect","params":{"path":"dist/printing-holes"}}]
[{"command":"plugin.run","params":{"path":"dist/printing-holes","approval":"<hash>","entry":"teardrop","params":{}}}]
```

Menü und Ribbon-Knopf gibt es ab SolidCraft 0.42.

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
